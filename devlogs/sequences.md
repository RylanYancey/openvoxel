
# The Problem 
We need to be able to transition between states, where the changes that need to occur to complete the transition may take more than a single frame. For example, loading of textures or connecting to a server would be sequences because they involve slow IO operations that need to happen in a specific order. While the program is doing this, we don't want it to freeze. That's very bad practice, and becomes very noticable if the user has a slow drive or an unstable internet connection. 

We need a system that allows us to schedule work, track its progress, and (optionally) inform the user of what's going on under the hood. We need the heavy lifting to happen in a separate thread (or task) so the user doesn't experience noticable stutter. 

# Rivulets
I wasn't sure what to call tasks within a sequence, since "task" is already used by the bevy_task crate, and a single unit could have multiple async tasks. So I didn't want to call it "task". So I asked claude and it gave some suggestions, Rivulet is a nice french word so I'm gonna use it.

A rivulet is a trackable unit of work within a sequence. So long as the `RivuletState` enum is not equal to `Finished`, the sequence will not be able to advance to the next stage. Rivulets also include a `progress` and `hint_text` field which can be used to inform the user of what work is occurring. 
```rs
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
```

Practically speaking, a rivulet is a system that runs in a stage of a sequence that adds itself in a registry (we'll talk more about that later) that prevents advancement until the rivulet is complete.

# The Sequences Trait
Sequences are enums that have an "inactive" variant and some number of active variants. When all the rivulets within a variant of a sequence are completed, the `Sequences::next()` function is called to advance to the next variant or complete the sequence. 
```rs
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
```

# The `Sequence<S>` Resource
When the next stage in the sequence is entered, systems that belong to that stage can register themselves in the Sequence resource. 
```rs
#[derive(Resource)]
pub struct Sequence<S> 
where
    S: Sequences,
{
    rivulets: RwLock<FxHashMap<String, Rivulet>>,
    /// used to inform client of why the sequence failed
    error: RwLock<Option<RivuletError>>,
    default: S,
}
```

# Example: ConnectSeq
This sequence outlines the steps that must occur, in-order, for a client to connect to a server. 
```rs
#[derive(Default, States, Eq, PartialEq, Debug, Clone, Hash)]
pub enum ConnectSeq {
    #[default]
    Inactive,
    Establishing,
    Authenticating,
    Syncronizing,
}

impl Sequences for ConnectSeq {
    fn is_active(&self) -> bool {
        *self != Self::Inactive
    }

    fn first() -> Self {
        Self::Establishing
    }

    fn next(&self) -> Option<Self> {
        match *self {
            Self::Inactive => Some(Self::Establishing),
            Self::Establishing => Some(Self::Authenticating),
            Self::Authenticating => Some(Self::Syncronizing),
            Self::Syncronizing => None,
        }
    }
}
```
