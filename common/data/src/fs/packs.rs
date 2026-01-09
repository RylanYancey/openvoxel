use std::{fs, io, path::PathBuf};

use bevy::{
    asset::ron::{self, de::SpannedError},
    prelude::*,
    tasks::{IoTaskPool, Task, futures_lite},
};
use fxhash::FxHashSet;
use serde::de::DeserializeOwned;

use crate::fs::path::{FileExt, get_relative_path_without_ext_as_string, iter_files_in_dir};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AssetPackId(pub u32);

#[derive(Resource, Default)]
pub struct AssetPackReader {
    next_mount_id: u32,
    mounts: Vec<Mount>,
}

impl AssetPackReader {
    /// Add an asset pack to the reader.
    pub fn mount(&mut self, path: PathBuf, priority: u32) -> AssetPackId {
        let id = AssetPackId(self.next_mount_id);
        self.next_mount_id += 1;

        let mount = Mount { path, priority, id };

        if let Some(i) = self.mounts.iter().position(|mt| mt.priority >= priority) {
            self.mounts.insert(i, mount);
        } else {
            self.mounts.push(mount);
        }

        id
    }

    /// Add an asset pack to the end of the reader.
    pub fn mount_to_end(&mut self, path: PathBuf) -> AssetPackId {
        let id = AssetPackId(self.next_mount_id);
        self.next_mount_id += 1;
        let priority = self.mounts.last().map(|mt| mt.priority).unwrap_or(0);
        self.mounts.push(Mount { path, priority, id });
        id
    }

    /// Load all files in the folder with the provided extensions.
    /// Higher priority packs will load their files first, and any other files
    /// with the same relative path as an already loaded file won't be loaded again.
    /// Use the "allow_meta" field to control whether metadata will be checked for.
    pub fn load_folder(
        &self,
        rel: impl AsRef<str>,
        exts: &[FileExt],
        allow_meta: bool,
    ) -> PackFolder {
        let rel = rel.as_ref().to_string();
        let task_pool = IoTaskPool::get();
        let mut in_progress = FxHashSet::<String>::default();
        let mut tasks = Vec::new();

        for mount in &self.mounts {
            let pack_id = mount.id;
            let folder_path = mount.path.join(&rel);
            for path in iter_files_in_dir(&folder_path) {
                let ext = FileExt::from(&path);
                if let Some(rel) = get_relative_path_without_ext_as_string(&folder_path, &path)
                    && exts.contains(&ext)
                    && in_progress.insert(rel.clone())
                {
                    tasks.push(
                        task_pool.spawn(async move {
                            PackFile::load(path, ext, rel, pack_id, allow_meta)
                        }),
                    );
                }
            }
        }

        PackFolder {
            rel,
            total: tasks.len(),
            tasks,
        }
    }
}

struct Mount {
    path: PathBuf,
    priority: u32,
    id: AssetPackId,
}

#[derive(Default)]
pub struct PackFolder {
    /// Relative path of folder.
    pub rel: String,

    /// Total number of files to read.
    pub total: usize,

    /// In-progress file loads.
    tasks: Vec<Task<Result<PackFile, FileError>>>,
}

impl PackFolder {
    /// Number of completed files divided by total number of files.
    /// In the range [0.0,1.0]
    pub fn progress(&self) -> f32 {
        if self.tasks.len() == 0 {
            1.0
        } else {
            self.tasks.len() as f32 / self.total as f32
        }
    }

    /// Number of files that have been consumed by the ready iterator.
    pub fn amount_finished(&self) -> usize {
        self.total - self.tasks.len()
    }

    /// Get all files that are have been successfully read.
    /// Will yield at most `limit` files.
    pub fn ready<'a>(&'a mut self, limit: usize) -> PackFolderReady<'a> {
        PackFolderReady {
            yielded: 0,
            limit,
            curr: 0,
            folder: self,
        }
    }
}

pub struct PackFolderReady<'a> {
    /// Folder to be read
    folder: &'a mut PackFolder,
    /// Number of files yielded this pass.
    yielded: usize,
    /// Max number of files that can be yielded at once.
    limit: usize,
    /// task index cursor
    curr: usize,
}

impl<'a> Iterator for PackFolderReady<'a> {
    type Item = Result<PackFile, FileError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.yielded < self.limit {
            while self.curr < self.folder.tasks.len() {
                if self.folder.tasks[self.curr].is_finished() {
                    self.yielded += 1;
                    let task = self.folder.tasks.swap_remove(self.curr);
                    return Some(futures_lite::future::block_on(task));
                } else {
                    self.curr += 1;
                }
            }
        }

        None
    }
}

#[derive(Debug)]
pub struct FileError {
    /// Absolute path of the file data.
    path: PathBuf,

    /// The pack in which the file tried to load.
    pack: AssetPackId,

    /// Whether the error comes from reading the file data or the meta.
    kind: FileErrorKind,
}

#[derive(Debug)]
pub enum FileErrorKind {
    FileReadFailed(io::Error),
    MetaReadFailed(io::Error),
}

pub struct PackFile {
    /// The asset pack the file belongs to.
    pub pack: AssetPackId,

    /// The path of the file relative to the loaded folder.
    pub rel: String,

    /// Extension on the file data.
    pub ext: FileExt,

    /// The data associated with the file.
    pub data: Vec<u8>,

    /// The optional metadata of the file, if enabled.
    pub meta: Option<Vec<u8>>,
}

impl PackFile {
    pub fn load(
        path: PathBuf,
        ext: FileExt,
        rel: String,
        pack: AssetPackId,
        has_meta: bool,
    ) -> Result<Self, FileError> {
        // read main file content
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(e) => {
                return Err(FileError {
                    path,
                    pack,
                    kind: FileErrorKind::FileReadFailed(e),
                });
            }
        };

        let meta = if has_meta {
            // not all files loaded this way need metadata.
            None
        } else {
            // Metadata is optional, so if it's not found that's fine.
            let meta_path = path.with_extension("meta");
            match fs::read(&meta_path) {
                Ok(data) => Some(data),
                Err(e) => {
                    if matches!(e.kind(), io::ErrorKind::NotFound) {
                        None
                    } else {
                        return Err(FileError {
                            path,
                            pack,
                            kind: FileErrorKind::MetaReadFailed(e),
                        });
                    }
                }
            }
        };

        Ok(Self {
            pack,
            rel,
            ext,
            data,
            meta,
        })
    }

    pub fn deserialize_meta<T>(&self) -> Result<Option<T>, SpannedError>
    where
        T: DeserializeOwned,
    {
        if let Some(data) = &self.meta {
            match ron::de::from_bytes::<T>(&data) {
                Ok(v) => Ok(Some(v)),
                Err(e) => Err(e),
            }
        } else {
            Ok(None)
        }
    }
}
