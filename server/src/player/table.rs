use bevy::prelude::*;
use protocol::session::{Session, SessionMap};
use world::region::attribute::ChunkMask;

/// Map for resolving Sessions to player entities.
#[derive(Resource, Default)]
pub struct Players(SessionMap<Entry>);

impl Players {
    pub fn entity(&self, session: Session) -> Option<Entity> {
        self.0.get(session).map(|entry| entry.entity)
    }

    pub fn insert(&mut self, session: Session, entity: Entity) {
        self.0.insert(session, Entry { entity });
    }

    pub fn remove(&mut self, session: Session) -> Option<Entry> {
        self.0.remove(session)
    }
}

pub struct Entry {
    pub entity: Entity,
}
