use bevy::{prelude::*, state::state::FreelyMutableState};
use std::{
    io,
    path::{Path, PathBuf},
};

pub fn transition<S: FreelyMutableState + Clone>(to: S) -> impl FnMut(ResMut<NextState<S>>) {
    move |mut next: ResMut<NextState<S>>| {
        next.set(to.clone());
    }
}

pub fn load_assets_from_folder<P: AsRef<Path>, A: Asset>(
    assets: &AssetServer,
    folder: P,
) -> io::Result<Vec<Handle<A>>> {
    let mut entries = Vec::new();
    info!("Block Texture Folder: {}", folder.as_ref().display());
    for entry in std::fs::read_dir(folder)? {
        if let Ok(entry) = entry
            && entry.file_type().is_ok_and(|ft| ft.is_file())
        {
            info!("Block texture entry path: {}", entry.path().display());
            let path = format!("file:/{}", entry.path().display());
            entries.push(assets.load(path));
        }
    }

    Ok(entries)
}
