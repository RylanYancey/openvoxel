use bevy::{
    ecs::{
        entity::EntityHashMap,
        intern::{Interned, Interner},
    },
    prelude::*,
};
use fxhash::FxHashMap;

#[derive(Clone, Component, Default)]
pub struct Last<T>(pub T);

#[derive(Component, Clone, Copy, Deref)]
pub struct UiLabel(Interned<str>);

impl UiLabel {
    pub fn new(name: impl AsRef<str>) -> Self {
        static INTERNER: Interner<str> = Interner::new();
        Self(INTERNER.intern(name.as_ref()))
    }
}

#[derive(Resource, Default)]
pub struct UiLabels {
    resolver: FxHashMap<Interned<str>, Entity>,
    by_entity: EntityHashMap<Interned<str>>,
}

impl UiLabels {
    pub fn resolve(&self, label: &UiLabel) -> Option<Entity> {
        self.resolver.get(&label.0).copied()
    }
}

pub fn on_ui_label_remove(mut reader: RemovedComponents<UiLabel>, mut labels: ResMut<UiLabels>) {
    for entity in reader.read() {
        if let Some(label) = labels.by_entity.remove(&entity) {
            labels.resolver.remove(&label);
        }
    }
}

pub fn on_ui_label_add(
    query: Query<(Entity, &UiLabel), Added<UiLabel>>,
    mut labels: ResMut<UiLabels>,
) {
    for (entity, label) in &query {
        labels.resolver.insert(label.0, entity);
        labels.by_entity.insert(entity, label.0);
    }
}
