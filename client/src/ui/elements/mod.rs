use bevy::prelude::*;

/// Indicates an element can be selected
#[derive(Component)]
pub struct Selectable {}

/// Indicates the element is selected.
#[derive(Component, Copy, Clone)]
pub struct Selected;
