use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Default, Serialize, Deserialize)]
pub struct Session(pub u64);

impl Session {
    pub const ZERO: Self = Self(0);

    #[inline]
    pub fn new(index: usize, tag: u64) -> Self {
        Self((index & 0xFFF) as u64 | (tag << 12))
    }

    pub fn index(self) -> usize {
        (self.0 & 0xFFF) as usize
    }

    pub fn tag(self) -> u64 {
        self.0 & !0xFFF
    }

    pub fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }
}

impl std::hash::Hash for Session {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.tag());
    }
}

#[derive(Clone, Debug, Hash)]
pub struct SessionMap<V> {
    slots: Vec<Slot<V>>,
    len: usize,
}

impl<V> SessionMap<V> {
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            len: 0,
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn get(&self, session: Session) -> Option<&V> {
        self.slots
            .get(session.index())
            .and_then(|slot| match &slot {
                Slot::Occupied { key, value } if *key == session => Some(value),
                _ => None,
            })
    }

    #[inline]
    pub fn get_mut(&mut self, session: Session) -> Option<&mut V> {
        self.slots
            .get_mut(session.index())
            .and_then(|slot| match slot {
                Slot::Occupied { key, value } if *key == session => Some(value),
                _ => None,
            })
    }

    pub fn get_or_insert(&mut self, session: Session, mut f: impl FnMut() -> V) -> &mut V {
        if !self.contains(session) {
            self.insert(session, (f)());
        }

        self.get_mut(session).unwrap()
    }

    pub fn insert(&mut self, session: Session, value: V) -> Option<(Session, V)> {
        let i = session.index();
        if i <= self.slots.len() {
            self.slots.resize_with(i + 1, || Slot::Empty);
        }

        match std::mem::replace(
            &mut self.slots[i],
            Slot::Occupied {
                key: session,
                value,
            },
        ) {
            Slot::Occupied { key, value } if key != session => Some((key, value)),
            Slot::Empty => {
                self.len += 1;
                None
            }
            _ => unreachable!(),
        }
    }

    pub fn remove(&mut self, session: Session) -> Option<V> {
        if let Some(slot) = self.slots.get_mut(session.index()) {
            if let Slot::Occupied { key, .. } = slot
                && *key == session
            {
                match std::mem::replace(slot, Slot::Empty) {
                    Slot::Occupied { value, .. } => {
                        self.len -= 1;
                        self.pop_empty_slots();
                        return Some(value);
                    }
                    _ => unreachable!(),
                }
            }
        }

        None
    }

    #[inline]
    pub fn contains(&self, session: Session) -> bool {
        self.slots
            .get(session.index())
            .is_some_and(|slot| match slot {
                Slot::Occupied { key, .. } if *key == session => true,
                _ => false,
            })
    }

    #[inline]
    pub fn iter<'a>(&'a self) -> Iter<'a, V> {
        self.into_iter()
    }

    #[inline]
    pub fn iter_mut<'a>(&'a mut self) -> IterMut<'a, V> {
        self.into_iter()
    }

    #[inline]
    fn pop_empty_slots(&mut self) {
        while let Some(_) = self.slots.pop_if(|slot| slot.is_empty()) {}
    }
}

#[derive(Clone, Debug, Hash)]
enum Slot<V> {
    Empty,
    Occupied { key: Session, value: V },
}

impl<V> Slot<V> {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Occupied { .. } => false,
        }
    }
}

impl<V> Default for SessionMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, V> IntoIterator for &'a SessionMap<V> {
    type IntoIter = Iter<'a, V>;
    type Item = (Session, &'a V);

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            slots: self.slots.iter(),
        }
    }
}

impl<'a, V> IntoIterator for &'a mut SessionMap<V> {
    type IntoIter = IterMut<'a, V>;
    type Item = (Session, &'a mut V);

    fn into_iter(self) -> Self::IntoIter {
        IterMut {
            slots: self.slots.iter_mut(),
        }
    }
}

pub struct Iter<'a, V> {
    slots: std::slice::Iter<'a, Slot<V>>,
}

impl<'a, V> Iterator for Iter<'a, V> {
    type Item = (Session, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(slot) = self.slots.next() {
            if let Slot::Occupied { key, value } = slot {
                return Some((*key, value));
            }
        }
        None
    }
}

pub struct IterMut<'a, V> {
    slots: std::slice::IterMut<'a, Slot<V>>,
}

impl<'a, V> Iterator for IterMut<'a, V> {
    type Item = (Session, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(slot) = self.slots.next() {
            if let Slot::Occupied { key, value } = slot {
                return Some((*key, value));
            }
        }
        None
    }
}
