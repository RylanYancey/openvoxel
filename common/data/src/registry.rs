use std::{hash::Hash, mem::MaybeUninit};

use bevy::{
    ecs::intern::Interner,
    prelude::{Deref, DerefMut, Resource},
};
use fxhash::{FxHashMap, FxHashSet};
use protocol::ChannelId;

/// An index of an entry in a Registry.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct RegistryId(pub usize);

impl From<usize> for RegistryId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl Into<usize> for RegistryId {
    fn into(self) -> usize {
        self.0
    }
}

impl From<ChannelId> for RegistryId {
    fn from(value: ChannelId) -> Self {
        Self(value.0 as usize)
    }
}

impl Into<ChannelId> for RegistryId {
    fn into(self) -> ChannelId {
        ChannelId(self.0)
    }
}

/// A data structure for T that can be indexed or searched by name.
/// Entries can be inserted into Registries, but not removed.
/// The location of Entries can change to synchronize with a remote
/// server or world file.
#[derive(Resource)]
pub struct Registry<T>
where
    T: Send + Sync + 'static,
{
    entries: Vec<Entry<T>>,
    resolver: FxHashMap<&'static str, usize>,
}

impl<T> Registry<T>
where
    T: Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, item: T) -> RegistryId {
        let name = name.into();
        if let Some(id) = self.resolver.get(&*name) {
            self.entries[*id].item = item;
            RegistryId(*id)
        } else {
            let name = REGISTRY_NAME_INTERNER.intern(&*name).0;
            let id = RegistryId(self.entries.len());
            self.entries.push(Entry { id, name, item });
            self.resolver.insert(name, id.0);
            id
        }
    }

    /// Returns the provided item as an Err(T) if an entry with the name already exists.
    /// Otherwise, the assigned ID is returned in an Ok(id).
    pub fn insert_nonoverwriting(
        &mut self,
        name: impl Into<String>,
        item: T,
    ) -> Result<RegistryId, T> {
        let s = name.into();
        if self.resolver.contains_key(s.as_str()) {
            return Err(item);
        }

        let name = REGISTRY_NAME_INTERNER.intern(&*s).0;
        let id = RegistryId(self.entries.len());
        self.entries.push(Entry { item, name, id });
        self.resolver.insert(name, id.0);
        Ok(id)
    }

    /// Get the entry with this ID, if it exists.
    pub fn get(&self, id: impl Into<RegistryId>) -> Option<&Entry<T>> {
        self.entries.get(id.into().0)
    }

    /// Get the entry with this ID mutably, if it exists.
    pub fn get_mut(&mut self, id: impl Into<RegistryId>) -> Option<&mut Entry<T>> {
        self.entries.get_mut(id.into().0)
    }

    /// Get the entry with this name if it exists.
    pub fn get_by_name(&self, name: impl AsRef<str>) -> Option<&Entry<T>> {
        self.resolve(name).map(|id| &self.entries[id.0])
    }

    /// Resolve an entry name to a RegistryId.
    #[inline]
    pub fn resolve(&self, name: impl AsRef<str>) -> Option<RegistryId> {
        self.resolver.get(name.as_ref()).map(|id| RegistryId(*id))
    }

    /// Get a vector of registry names.
    pub fn get_names(&self) -> Vec<&'static str> {
        self.entries
            .iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>()
    }

    /// Attempt to make the registry compliant to another.
    pub fn make_compliant<A: AsRef<str> + Clone + Eq + PartialEq + Hash>(
        &mut self,
        to: &Vec<A>,
    ) -> Result<(), RegistryComplianceErrors<A>> {
        // Buffer of entry sources to entry destinations.
        let mut swap = Vec::<(usize, usize)>::with_capacity(self.entries.len());

        // check for entries in the server registry that aren't present on the client.
        // We don't care if the client has entries that the server doesn't have.
        let mut mismatches = FxHashSet::default();
        let mut duplicates = FxHashSet::<A>::default();

        // buffer of keys that haven't been visited.
        let mut remaining = FxHashSet::from_iter(self.resolver.keys().copied());

        for (i, key) in to.iter().enumerate() {
            if let Some(j) = self.resolver.get(key.as_ref()) {
                if !remaining.remove(key.as_ref()) {
                    duplicates.insert(key.clone());
                }

                swap.push((*j, i));
            } else {
                mismatches.insert(key.clone());
            }
        }

        // early return if a mismatch occurs.
        if !mismatches.is_empty() || !duplicates.is_empty() {
            return Err(RegistryComplianceErrors {
                mismatches,
                duplicates,
            });
        }

        // handle entries that exist locally but not on the remote.
        // This is allowed, by appending those entries to the end of the buffer.
        let mut base = swap.len();
        for rem in remaining {
            let i = *self.resolver.get(&*rem).unwrap();
            swap.push((i, base));
            base += 1;
        }

        // sort by destination index.
        swap.sort_by_key(|(_, dst)| *dst);

        // move old entries into new buffer.
        let old = vec_to_uninit(std::mem::take(&mut self.entries));
        let mut new = Vec::<Entry<T>>::with_capacity(swap.len());
        for (src, dst) in swap {
            new.push(unsafe { old[src].assume_init_read() });
            new[dst].id = RegistryId(dst);
            *self.resolver.get_mut(&new[dst].name).unwrap() = dst;
        }
        self.entries = new;

        Ok(())
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &Entry<T>> {
        self.entries.iter()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> {
        self.entries.iter().map(|entry| &entry.item)
    }

    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut T> {
        self.entries.iter_mut().map(|entry| &mut entry.item)
    }
}

impl<T> Default for Registry<T>
where
    T: Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            resolver: FxHashMap::default(),
        }
    }
}

#[derive(Deref, DerefMut)]
pub struct Entry<T> {
    #[deref]
    pub item: T,
    pub name: &'static str,
    pub id: RegistryId,
}

static REGISTRY_NAME_INTERNER: Interner<str> = Interner::new();

#[derive(Debug)]
pub struct RegistryComplianceErrors<A: AsRef<str> = String> {
    /// Registry entries that exist on the server but not on the client.
    pub mismatches: FxHashSet<A>,

    /// Registry entries that occurred more than once in the compliance buffer.
    pub duplicates: FxHashSet<A>,
}

fn vec_to_uninit<T>(vec: Vec<T>) -> Vec<MaybeUninit<T>> {
    let (ptr, len, cap) = vec.into_raw_parts();
    unsafe { Vec::from_raw_parts(ptr.cast::<MaybeUninit<T>>(), len, cap) }
}

#[cfg(test)]
mod tests {
    use super::Registry;

    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    struct Thing(u64);

    #[test]
    fn make_compliant() {
        let mut reg1 = Registry::<Thing>::new();
        reg1.insert("thing0", Thing(0));
        reg1.insert("thing1", Thing(1));
        reg1.insert("thing2", Thing(2));
        reg1.insert("thing3", Thing(3));
        reg1.insert("thing4", Thing(4));

        let mut reg2 = Registry::<Thing>::new();
        reg2.insert("thing4", Thing(4));
        reg2.insert("thing2", Thing(2));
        reg2.insert("thing3", Thing(3));
        reg2.insert("thing1", Thing(1));
        reg2.insert("thing0", Thing(0));

        let names1 = reg1.get_names();
        reg2.make_compliant(&names1).unwrap();

        for (e1, e2) in reg1.entries().zip(reg2.entries()) {
            assert_eq!(e1.name, e2.name);
            assert_eq!(e1.item, e2.item);
            assert_eq!(e1.id, e2.id);
        }
    }

    #[test]
    #[should_panic]
    fn make_compliant_fail_on_mismatch() {
        let mut reg1 = Registry::<Thing>::new();
        reg1.insert("thing0", Thing(0));
        reg1.insert("thing1", Thing(1));
        reg1.insert("thing2", Thing(2));
        reg1.insert("thing3", Thing(3));
        reg1.insert("thing4", Thing(4));

        // reg2 does not have thing3
        let mut reg2 = Registry::<Thing>::new();
        reg2.insert("thing4", Thing(4));
        reg2.insert("thing2", Thing(2));
        reg2.insert("thing0", Thing(0));
        reg2.insert("thing1", Thing(1));

        let names1 = reg1.get_names();
        reg2.make_compliant(&names1).unwrap();
    }
}
