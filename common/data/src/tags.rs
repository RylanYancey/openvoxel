use std::{
    alloc::{Allocator, Global},
    cmp::Ordering,
    hash::Hash,
    ops::Index,
};

use bevy::ecs::intern::{Internable, Interned, Interner};

use crate::registry::RegistryId;

/// u16 key for a Tag.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct Tag(pub u16);

impl Tag {
    pub const NULL: Self = Self(u16::MAX);
}

impl Default for Tag {
    fn default() -> Self {
        Self::NULL
    }
}

impl From<RegistryId> for Tag {
    fn from(value: RegistryId) -> Self {
        Self(value.0 as u16)
    }
}

impl Into<RegistryId> for Tag {
    fn into(self) -> RegistryId {
        RegistryId(self.0 as usize)
    }
}

/// Null-terminated, growable,sorted set of Tags.
/// Sorting is used instead of hashing because we need
/// to check the set for intersections. They are also
/// relatively small so it shouldn't matter.
#[derive(Clone, Debug)]
pub struct TagSet<A: Allocator = Global>(Vec<Tag, A>);

impl TagSet<Global> {
    #[inline]
    pub fn new() -> Self {
        Self::new_in(Global)
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_in(capacity, Global)
    }
}

impl<A: Allocator> TagSet<A> {
    #[inline]
    pub fn new_in(alloc: A) -> Self {
        Self::with_capacity_in(1, alloc)
    }

    #[inline]
    pub fn with_capacity_in(capacity: usize, alloc: A) -> Self {
        let mut vec = Vec::with_capacity_in(capacity, alloc);
        vec.push(Tag::NULL);
        Self(vec)
    }

    pub const fn as_slice<'a>(&'a self) -> &'a TagSlice {
        unsafe { std::mem::transmute::<&'a [Tag], &'a TagSlice>(self.0.as_slice()) }
    }

    /// Whether or not the tag exists in the set.
    /// Always returns true if the tag is Tag::NULL.
    pub const fn contains(&self, tag: Tag) -> bool {
        self.as_slice().contains(tag)
    }

    /// Insert the tag into the set, returning whether
    /// or not the tag already existed in the set.
    /// Returns false if the tag is Tag::NULL.
    pub fn insert(&mut self, tag: Tag) -> bool {
        if tag == Tag::NULL {
            return false;
        }
        let i = self.as_slice().sort_index(tag);
        if self.0[i] != tag {
            self.0.insert(i, tag);
            // tag not exist
            false
        } else {
            // tag already exists
            true
        }
    }

    /// Remove a tag from the set, returning whether or not the removal was successful.
    /// Returns false if the tag is Tag::NULL.
    pub fn remove(&mut self, tag: Tag) -> bool {
        if tag == Tag::NULL {
            return false;
        }
        if let Some(i) = self.as_slice().index_of(tag) {
            self.0.remove(i);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn iter<'a>(&'a self) -> Iter<'a> {
        self.into_iter()
    }
}

impl Default for TagSet<Global> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Allocator> AsRef<[Tag]> for TagSet<A> {
    fn as_ref(&self) -> &[Tag] {
        &self.0
    }
}

impl<'a, A: Allocator> IntoIterator for &'a TagSet<A> {
    type IntoIter = Iter<'a>;
    type Item = Tag;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            tags: &self.0,
            curr: 0,
        }
    }
}

impl<A: Allocator> Eq for TagSet<A> {}
impl<A: Allocator> PartialEq for TagSet<A> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice().eq(&other.as_slice())
    }
}

impl<A: Allocator> Hash for TagSet<A> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state)
    }
}

/// Null-terminated and sorted slice of tags.
#[derive(Debug)]
pub struct TagSlice([Tag]);

impl TagSlice {
    /// Compute whether this set contains the tag.
    #[inline]
    pub const fn contains(&self, tag: Tag) -> bool {
        self.index_of(tag).is_some()
    }

    /// Compute whether self and other have any matching tags.
    #[inline]
    pub fn intersects(&self, other: &TagSlice) -> bool {
        let (mut i, mut j) = (0, 0);
        loop {
            // i and j is known to be in-bounds because tag slices
            // always have at least one element.
            let u = unsafe { *self.0.get_unchecked(i) };
            let v = unsafe { *other.0.get_unchecked(j) };

            // return false if u or v is a null tag. (termination)
            // Technically, if u was 0x7FFF and v was 0x8000, this would
            // return true even though neither u or v was null, but since
            // the tag is based on an index this shouldn't happen
            // unless there was more than 32767 tags.
            if u.0 | v.0 == u16::MAX {
                return false;
            }

            match u.cmp(&v) {
                // match found, return true.
                Ordering::Equal => return true,
                // self > other, incremenet other index
                Ordering::Greater => j += 1,
                // self < other, increment self index.
                Ordering::Less => i += 1,
            }
        }
    }

    #[inline]
    pub fn iter<'a>(&'a self) -> Iter<'a> {
        self.into_iter()
    }

    /// Index where the tag could be inserted.
    /// Returns None if the the tag already exists,
    /// or there are no available slots in the set.
    const fn sort_index(&self, tag: Tag) -> usize {
        let mut i = 0;
        loop {
            if self.0[i].0 >= tag.0 {
                return i;
            }
            i += 1;
        }
    }

    /// Index of the tag in the set.
    const fn index_of(&self, tag: Tag) -> Option<usize> {
        let mut i = 0;
        loop {
            if self.0[i].0 >= tag.0 {
                return if tag.0 == self.0[i].0 { Some(i) } else { None };
            }
            i += 1;
        }
    }

    /// Truncate the slice so there is only one null at the end of the buffer.
    /// This can happen when getting a FixedTagSet as a slice.
    pub fn without_extra_nulls<'a>(&'a self) -> &'a TagSlice {
        let i = self.index_of(Tag::NULL).unwrap();
        let slice = &self.0[..i + 1];
        unsafe { std::mem::transmute::<&[Tag], &TagSlice>(slice) }
    }

    /// Intern the value as a TagSlice.
    pub fn intern(&self) -> Interned<TagSlice> {
        static INTERNER: Interner<TagSlice> = Interner::new();
        INTERNER.intern(self.without_extra_nulls())
    }
}

impl<'a> AsRef<[Tag]> for &'a TagSlice {
    fn as_ref(&self) -> &[Tag] {
        &self.0[..self.0.len() - 1]
    }
}

impl<'a> IntoIterator for &'a TagSlice {
    type IntoIter = Iter<'a>;
    type Item = Tag;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            tags: &self.0,
            curr: 0,
        }
    }
}

impl Internable for TagSlice {
    fn leak(&self) -> &'static Self {
        let mut vec = Vec::from_iter(self);
        vec.push(Tag::NULL);
        let slice = Box::leak(vec.into_boxed_slice());
        unsafe { std::mem::transmute::<&'static [Tag], &'static TagSlice>(slice) }
    }

    fn ref_eq(&self, other: &Self) -> bool {
        std::ptr::eq(self as *const _, other as *const _)
    }

    fn ref_hash<H: core::hash::Hasher>(&self, state: &mut H) {
        let ptr = unsafe { std::mem::transmute::<&Self, &[Tag]>(self).as_ptr() };
        state.write_usize(ptr as usize)
    }
}

impl Hash for TagSlice {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let mut i = 0;
        loop {
            let v = unsafe { *self.0.get_unchecked(i) };
            if v == Tag::NULL {
                return;
            }
            state.write_u16(v.0);
            i += 1;
        }
    }
}

impl Eq for TagSlice {}
impl PartialEq for TagSlice {
    fn eq(&self, other: &Self) -> bool {
        let mut i = 0;
        loop {
            let u = unsafe { self.0.get_unchecked(i) };
            let v = unsafe { other.0.get_unchecked(i) };

            if u != v {
                return false;
            }

            if u.0 | v.0 == u16::MAX {
                return true;
            }

            i += 1;
        }
    }
}

/// Null-terminated set of tags with a fixed capacity.
///
/// The buffer must be sorted in ascending order.
/// Unused slots must be set to Tag::Null.
#[derive(Copy, Clone, Debug)]
pub struct FixedTagSet<const N: usize>([Tag; N]);

impl<const N: usize> FixedTagSet<N> {
    pub const NULL: Self = Self([Tag::NULL; N]);

    /// Construct a new Tag Set with capacity N.
    pub const fn new() -> Self {
        Self::NULL
    }

    /// Construct a fixed tag set from a tag slice.
    /// Only N elements will be taken, any additional elements will be unused.
    /// Length of the slice does not have to equal this set's capacity.
    pub fn from_slice(s: impl AsRef<[Tag]>) -> Self {
        Self::from_iter(s.as_ref().iter().copied())
    }

    /// Construct a fixed tag set from an iterator.
    /// Only N-1 elements will be taken, any additional elements will be discarded.
    /// Length of the iterator does not have to equal this set's capacity.
    pub fn from_iter(it: impl IntoIterator<Item = Tag>) -> Self {
        let mut ret = Self::NULL;
        for (i, tag) in it.into_iter().take(N - 1).enumerate() {
            ret.0[i] = tag;
        }
        ret.0.sort_unstable();
        ret
    }

    /// Get the set as a TagSlice.
    /// The returned slice is not truncated to the last null value, so there
    /// may be more than one null tag at the end of the slice.
    pub const fn as_slice<'a>(&'a self) -> &'a TagSlice {
        unsafe { std::mem::transmute::<&'a [Tag], &'a TagSlice>(self.0.as_slice()) }
    }

    /// Number of non-null tags present in the set.
    pub fn len(&self) -> usize {
        self.as_slice().index_of(Tag::NULL).unwrap()
    }

    /// Insert a tag into the set.
    /// If the set overflows, a tag will be removed,
    /// which may be the inserted tag or the tag at
    /// the end of the buffer, whichever is larger.
    pub fn insert(&mut self, tag: Tag) -> Option<Tag> {
        if tag == Tag::NULL {
            // don't insert null tags.
            return None;
        };
        let i = self.as_slice().sort_index(tag);
        if self.0[i] == tag || i == N - 1 {
            // tag already exists or set is full.
            return None;
        }

        // make space for element.
        self.0.copy_within(i..N - 1, i + 1);

        // assign tag value
        self.0[i] = tag;

        // ensure last element is null.
        if self.0[N - 1] != Tag::NULL {
            // pop non-null termination
            Some(std::mem::replace(&mut self.0[N - 1], Tag::NULL))
        } else {
            None
        }
    }

    /// Remove a tag from the buffer, returning whether it existed.
    pub fn remove(&mut self, tag: Tag) -> bool {
        if let Some(i) = self.as_slice().index_of(tag) {
            if i != N - 1 {
                self.0.copy_within(i + 1..N, i);
            }
            self.0[N - 1] = Tag::NULL;
            true
        } else {
            false
        }
    }

    /// Whether or not self and other share any non-null tags.
    pub fn intersects(&self, other: &TagSlice) -> bool {
        self.as_slice().intersects(other)
    }

    /// Whether or not the tag is in this set.
    pub fn contains(&self, tag: Tag) -> bool {
        self.as_slice().contains(tag)
    }

    /// Get an iterator over the non-null tags in the set.
    #[inline]
    pub fn iter<'a>(&'a self) -> Iter<'a> {
        self.into_iter()
    }
}

impl<const N: usize> Default for FixedTagSet<N> {
    fn default() -> Self {
        Self::NULL
    }
}

impl<const N: usize> Index<usize> for FixedTagSet<N> {
    type Output = Tag;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<const N: usize> AsRef<[Tag]> for FixedTagSet<N> {
    fn as_ref(&self) -> &[Tag] {
        &self.0
    }
}

impl<'a, const N: usize> IntoIterator for &'a FixedTagSet<N> {
    type IntoIter = Iter<'a>;
    type Item = Tag;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        Iter {
            tags: &self.0,
            curr: 0,
        }
    }
}

impl<const N: usize> Eq for FixedTagSet<N> {}
impl<const N: usize> PartialEq for FixedTagSet<N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice().eq(&other.as_slice())
    }
}

impl<const N: usize> Hash for FixedTagSet<N> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state)
    }
}

#[derive(Clone)]
pub struct Iter<'a> {
    /// Null-terminated, sorted tag slice.
    tags: &'a [Tag],
    curr: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = Tag;

    fn next(&mut self) -> Option<Self::Item> {
        let v = unsafe { *self.tags.get_unchecked(self.curr) };
        if v == Tag::NULL {
            None
        } else {
            self.curr += 1;
            Some(v)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tags::Tag;

    use super::FixedTagSet;

    const TAGS: [Tag; 7] = [Tag(6), Tag(4), Tag(5), Tag(2), Tag(3), Tag(1), Tag(0)];

    #[test]
    fn insert_remove() {
        let mut set = FixedTagSet::<8>::new();
        for tag in TAGS {
            assert_eq!(set.insert(tag), None);
        }
        let expectation = [
            Tag(0),
            Tag(1),
            Tag(2),
            Tag(3),
            Tag(4),
            Tag(5),
            Tag(6),
            Tag::NULL,
        ];
        assert_eq!(set.0, expectation);

        for tag in TAGS {
            assert!(set.remove(tag));
        }
        assert_eq!(set.0, [Tag::NULL; 8]);
    }

    #[test]
    fn iter() {
        let tags: [Tag; 7] = core::array::from_fn(|i| Tag(i as u16));
        let set = FixedTagSet::<8>::from_slice(&tags);
        for (i, tag2) in set.iter().enumerate() {
            assert_eq!(tags[i], tag2);
        }
    }

    #[test]
    fn contains() {
        let set = FixedTagSet::<8>::from_slice(TAGS);
        for tag in TAGS {
            assert!(set.contains(tag));
        }
    }

    #[test]
    fn intersects() {
        // shared index 3 and 0
        let set1 = FixedTagSet::<5>::from_slice(&[Tag(0), Tag(1), Tag(2), Tag(3)]);
        let set2 = FixedTagSet::<4>::from_slice(&[Tag(3), Tag(7)]);
        assert!(set1.intersects(set2.as_slice()));
        assert!(set2.intersects(set1.as_slice()));

        // shared index 0 and 1
        let set1 = FixedTagSet::<4>::from_slice(&[Tag(8), Tag(16), Tag(32), Tag::NULL]);
        let set2 = FixedTagSet::<4>::from_slice(&[Tag(0), Tag(8), Tag::NULL, Tag::NULL]);
        assert!(set1.intersects(set2.as_slice()));
        assert!(set2.intersects(set1.as_slice()));

        // shared index 1 and 2
        let set1 = FixedTagSet::<4>::from_slice(&[Tag(8), Tag(16), Tag(32), Tag::NULL]);
        let set2 = FixedTagSet::<4>::from_slice(&[Tag(0), Tag(9), Tag(16), Tag::NULL]);
        assert!(set1.intersects(set2.as_slice()));
        assert!(set2.intersects(set1.as_slice()));

        // no shared indices
        let set1 = FixedTagSet::<4>::from_slice(&[Tag(8), Tag(16), Tag(32), Tag::NULL]);
        let set2 = FixedTagSet::<4>::from_slice(&[Tag(9), Tag(17), Tag(33), Tag::NULL]);
        assert!(!set1.intersects(set2.as_slice()));
        assert!(!set2.intersects(set1.as_slice()));

        // Both sets null
        let set1 = FixedTagSet::<4>::new();
        let set2 = FixedTagSet::<4>::new();
        assert!(!set1.intersects(set2.as_slice()));
        assert!(!set2.intersects(set1.as_slice()));

        // shared index same, 2
        let set1 = FixedTagSet::<4>::from_slice(&[Tag(8), Tag(16), Tag(32), Tag::NULL]);
        let set2 = FixedTagSet::<4>::from_slice(&[Tag(0), Tag(9), Tag(32), Tag::NULL]);
        assert!(set1.intersects(set2.as_slice()));
        assert!(set2.intersects(set1.as_slice()));
    }
}
