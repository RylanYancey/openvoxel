#![feature(allocator_api)]
#![feature(slice_ptr_get)]
#![feature(iter_collect_into)]

use bevy::prelude::*;

use crate::locale::Locale;

pub mod blocks;
pub mod blockstates;
pub mod fs;
pub mod info;
pub mod locale;
pub mod queue;
pub mod registry;
pub mod sequence;
pub mod states;
pub mod table;
pub mod tags;
pub mod text;
pub mod util;

pub struct OpenvoxelDataPlugin;

impl Plugin for OpenvoxelDataPlugin {
    #[rustfmt::skip]
    fn build(&self, app: &mut App) {
        let root_path = info::RootPath::default();
        app
            // initialize resources
            // .insert_resource({
            //     let mut rdr = fs::packs::AssetPackReader::default();
            //     rdr.mount_to_end(root_path.join("assets/vanilla"));
            //     rdr
            // })
            .insert_resource(root_path)
            .init_resource::<info::Version>()
            .init_resource::<Locale>()
        ;
    }
}
