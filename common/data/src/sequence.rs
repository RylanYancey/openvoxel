use std::{io, marker::PhantomData};

use bevy::{prelude::*, state::state::FreelyMutableState};
use fxhash::FxHashMap;
use parking_lot::RwLock;

#[derive(Default)]
pub struct SequencesPlugin<S: Sequences>(pub S);

impl<S> Plugin for SequencesPlugin<S>
where
    S: Sequences,
{
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        app
            .insert_state(self.0.clone())
            .insert_resource(
                Sequence {
                    rivulets: RwLock::new(FxHashMap::default()),
                    default: self.0.clone(),
                    error: RwLock::default(),
                }
            )
            .add_message::<SequenceStarted<S>>()
            .add_message::<SequenceEnded<S>>()
            .add_message::<SequenceFailed<S>>()
            .add_systems(OnExit(self.0.clone()), (
                write_sequence_start_ev::<S>,
            ))
            .add_systems(PostUpdate, (
                advance_sequence_state::<S>
                    .run_if(not(in_state(self.0.clone()))),
            ))
        ;
    }
}

fn advance_sequence_state<S: Sequences>(
    seq: Res<Sequence<S>>,
    curr: Res<State<S>>,
    mut next: ResMut<NextState<S>>,
    mut end_evs: MessageWriter<SequenceEnded<S>>,
    mut err_evs: MessageWriter<SequenceFailed<S>>,
) {
    if curr.is_active() {
        if let Some(e) = seq.get_err() {
            warn!("Sequence failed with error: '{e:?}'");
            err_evs.write(SequenceFailed {
                error: e,
                stage: curr.get().clone(),
            });
            next.set(seq.default.clone());
        }

        if seq.is_empty_or_all_finished() {
            seq.clear();
            let new = if let Some(state) = curr.get().next() {
                state
            } else {
                end_evs.write(SequenceEnded::<S>(PhantomData));
                seq.default.clone()
            };
            next.set(new.clone());

            info!(
                "Transitioned sequence from '{:?}' to '{:?}'.",
                curr.get(),
                new
            );
        }
    }
}

fn write_sequence_start_ev<S: Sequences>(mut evs: MessageWriter<SequenceStarted<S>>) {
    evs.write(SequenceStarted(PhantomData));
}

#[derive(Message, Clone)]
pub struct SequenceStarted<S>(PhantomData<S>);

#[derive(Message, Clone)]
pub struct SequenceEnded<S>(PhantomData<S>);

#[derive(Message, Debug)]
pub struct SequenceFailed<S> {
    pub error: RivuletError,
    pub stage: S,
}

#[derive(Debug, Clone)]
pub struct RivuletError {
    pub err_code: &'static str,
    pub err_text: String,
}

#[derive(Resource)]
pub struct Sequence<S> {
    rivulets: RwLock<FxHashMap<String, Rivulet>>,
    error: RwLock<Option<RivuletError>>,
    default: S,
}

impl<S> Sequence<S> {
    pub fn all_finished<A: AsRef<str>>(&self, items: impl IntoIterator<Item = A>) -> bool {
        let rivulets = self.rivulets.read();
        items.into_iter().all(|item| {
            let key = item.as_ref();
            if let Some(v) = rivulets.get(key) {
                v.state == RivuletState::Finished
            } else {
                // if !self.is_first_tick {
                //     warn!("[D999] Attempted to check if rivulet with name '{key}' was finished, but it did not exist.");
                // }

                false
            }
        })
    }

    pub fn progress_in_stage(&self) -> f32 {
        let rivulets = self.rivulets.read();
        let progress = rivulets
            .values()
            .map(|rv| {
                if rv.state == RivuletState::Finished {
                    1.0
                } else {
                    rv.progress
                }
            })
            .sum::<f32>();
        progress / rivulets.len() as f32
    }

    pub fn get_in_progress<'a>(&'a self, rivulet: &'a str) -> Option<RivuletGuard<'a, S>> {
        let mut guard = self.get(rivulet);
        if let RivuletState::Finished = guard.state {
            None
        } else {
            guard.state = RivuletState::InProgress;
            Some(guard)
        }
    }

    pub fn get<'a>(&'a self, rivulet: &'a str) -> RivuletGuard<'a, S> {
        {
            let read_guard = self.rivulets.read();
            if let Some(v) = read_guard.get(rivulet) {
                return RivuletGuard {
                    sequence: self,
                    name: rivulet,
                    progress: v.progress,
                    state: v.state,
                };
            }
        }

        {
            let mut write_guard = self.rivulets.write();
            write_guard
                .entry(rivulet.into())
                .or_insert_with(|| Rivulet {
                    progress: 0.0,
                    hint_text: String::default(),
                    state: RivuletState::Uninit,
                });
        }

        info!("Starting rivulet with name: {rivulet}");
        RivuletGuard {
            sequence: self,
            name: rivulet,
            progress: 0.0,
            state: RivuletState::Uninit,
        }
    }

    pub fn set_error(&self, e: RivuletError) {
        *self.error.write() = Some(e);
    }

    pub fn get_err(&self) -> Option<RivuletError> {
        self.error.read().clone()
    }

    fn clear(&self) {
        self.rivulets.write().clear();
    }

    fn is_empty_or_all_finished(&self) -> bool {
        let guard = self.rivulets.read();
        for entry in guard.values() {
            if entry.state != RivuletState::Finished {
                return false;
            }
        }

        true
    }
}

/// A wrapper over a Rivulet in a Sequence.
pub struct RivuletGuard<'a, S> {
    sequence: &'a Sequence<S>,
    pub name: &'a str,
    pub progress: f32,
    pub state: RivuletState,
}

impl<'a, S> RivuletGuard<'a, S> {
    pub fn set_hint_text(&mut self, text: impl Into<String>) {
        if let Some(inner) = self.sequence.rivulets.write().get_mut(self.name) {
            inner.hint_text = text.into();
        }
    }
}

impl<'a, S> Drop for RivuletGuard<'a, S> {
    fn drop(&mut self) {
        if let Some(inner) = self.sequence.rivulets.write().get_mut(self.name) {
            inner.progress = self.progress.min(1.0);
            inner.state = self.state;
        }
    }
}

/// A unit of work within a stage of a sequence.
#[derive(Debug)]
struct Rivulet {
    /// How close the work is to completion.
    /// May not be used if the task is untrackable.
    progress: f32,

    /// What to show the user while the rivulet is in-progress.
    hint_text: String,

    /// The state of the rivulet, on of:
    ///  - Uninit
    ///  - InProgress
    ///  - Finished
    state: RivuletState,
}

/// The state of the Rivulet.
/// This is reset to uninit when the sequence
/// completes.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RivuletState {
    Uninit = 0,
    InProgress = 1,
    Finished = 2,
}

pub trait Sequences: Clone + FreelyMutableState {
    /// Whether this sequence is an active or inactive part.
    fn is_active(&self) -> bool;

    /// The first variant in the sequence.
    /// This should not be your Inactive variant, it
    /// should be the first active variant that triggers
    /// the sequence.
    fn first() -> Self;

    /// Get the next variant, returning None if the
    /// sequence is complete.
    fn next(&self) -> Option<Self>;
}
