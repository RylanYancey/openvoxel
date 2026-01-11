use bevy::prelude::*;
pub use priority_queue::PriorityQueue;
use std::{collections::VecDeque, hash::Hash};

/// A "Queue" resource that can have multiple different internal implementations.
#[derive(Resource, Deref, DerefMut)]
pub struct Queue<T: Queueable> {
    #[deref]
    inner: T::Inner,
}

impl<T: Queueable> Default for Queue<T> {
    fn default() -> Self {
        Self {
            inner: T::Inner::default(),
        }
    }
}

pub trait Queueable {
    type Inner: Default;
}

/// A very simple push/pop queue.
pub struct LinearQueue<T> {
    buffer: VecDeque<T>,
}

impl<T> LinearQueue<T> {
    /// Number of items queued.
    #[inline]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Push to the back of the queue.
    #[inline]
    pub fn add(&mut self, item: T) {
        self.buffer.push_back(item);
    }

    /// Pop off the front of the queue.
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.buffer.pop_front()
    }
}

impl<T> Default for LinearQueue<T> {
    fn default() -> Self {
        Self {
            buffer: VecDeque::new(),
        }
    }
}

/// A queue where duplicate items increase
/// the priority of the existing item instead of adding.
pub struct RepetitionPriorityQueue<T>
where
    T: Hash + Eq,
{
    buffer: PriorityQueue<T, u64>,
}

impl<T> RepetitionPriorityQueue<T>
where
    T: Hash + Eq,
{
    /// The number of items in the queue.
    #[inline]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Push an item onto the Queue.
    /// If the item already exists, its priority is
    /// increased and "true" is returned.
    #[inline]
    pub fn push(&mut self, item: T) -> bool {
        self.push_with_priority(item, 1)
    }

    /// Push an item onto the Queue with an assigned priority.
    /// If the item already exists, its priority is
    /// increased and "true" is returned.
    #[inline]
    pub fn push_with_priority(&mut self, item: T, prio: u64) -> bool {
        self.buffer.push_increase(item, prio).is_some()
    }

    /// Pop the highest priority item off the queue.
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.buffer.pop().map(|(item, _)| item)
    }

    /// Extract all items for which the predicate returns true.
    pub fn extract_if(
        &mut self,
        predicate: impl FnMut(&mut T, &mut u64) -> bool,
    ) -> impl Iterator<Item = T> {
        self.buffer.extract_if(predicate).map(|(item, _)| item)
    }
}

impl<T> Default for RepetitionPriorityQueue<T>
where
    T: Hash + Eq,
{
    fn default() -> Self {
        Self {
            buffer: PriorityQueue::default(),
        }
    }
}
