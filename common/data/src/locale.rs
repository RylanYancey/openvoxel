use std::fs;
use std::{io, path::Path};

use bevy::asset::ron;
use bevy::prelude::*;
use fxhash::FxHashMap;

#[derive(Resource)]
pub struct Locale {
    map: FxHashMap<String, String>,
}

impl Locale {
    pub fn get(&self, label: impl AsRef<str>) -> String {
        let label = label.as_ref();
        self.map.get(label).cloned().unwrap_or_else(|| label.into())
    }

    pub fn load_file(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let data = fs::read(path)?;
        match ron::de::from_bytes::<FxHashMap<String, String>>(&data) {
            Err(e) => {
                error!("[D110] Failed to deserialize Localization file with error: '{e}'");
            }
            Ok(map) => {
                for (key, val) in map {
                    self.map.insert(key, val);
                }
            }
        }
        Ok(())
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self {
            map: FxHashMap::default(),
        }
    }
}
