use bevy::{prelude::*, render::storage::ShaderStorageBuffer};
use data::{
    info::RootPath,
    sequence::{RivuletState, Sequence, Sequences},
    util::load_assets_from_folder,
};

#[derive(States, Eq, PartialEq, Debug, Default, Clone, Hash)]
pub enum StartupSeq {
    #[default]
    Inactive,
    LoadTextures,
    BuildTextureArrays,
}

impl StartupSeq {
    /// Returns the current stage index and the total number of stages.
    pub fn stage_index(&self) -> Option<(usize, usize)> {
        let c = match *self {
            Self::Inactive => return None,
            Self::LoadTextures => 0,
            Self::BuildTextureArrays => 1,
        };
        Some((c, 2))
    }
}

impl Sequences for StartupSeq {
    fn is_active(&self) -> bool {
        *self != Self::Inactive
    }

    fn first() -> Self {
        Self::LoadTextures
    }

    fn next(&self) -> Option<Self> {
        Some(match *self {
            Self::LoadTextures => Self::BuildTextureArrays,
            _ => return None,
        })
    }
}
