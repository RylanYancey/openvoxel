use bevy::prelude::*;
use std::path::{Path, PathBuf};

#[derive(Resource, Deref)]
pub struct Version(pub String);

impl Default for Version {
    fn default() -> Self {
        Self("UNKNOWN".into())
    }
}

#[derive(Clone, Resource, Deref)]
pub struct RootPath(&'static Path);

impl RootPath {
    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.0.join(path.as_ref())
    }
}

impl Default for RootPath {
    fn default() -> Self {
        #[cfg(feature = "dev")]
        {
            let path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
            info!("Selecting root path: {}", path.display());
            return Self(PathBuf::leak(path));
        }
    }
}
