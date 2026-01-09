use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use bevy::{log::error, prelude::Deref};
use serde::Deserialize;

/// Get the path relative to the mount given a subpath and convert to String.
/// Example:
///  - mount="/silly/billy"
///  - sub="/silly/billy/willy/milly.png"
///  - returns: Some("willy/milly")
pub fn get_relative_path_without_ext_as_string(mount: &Path, sub: &Path) -> Option<String> {
    Some(
        sub.strip_prefix(mount)
            .ok()?
            .with_extension("")
            .to_str()?
            .to_owned(),
    )
}

/// Iterate file paths in a directory, non-recursively.
/// Yields the absolute path. To get the relative path, use `Path::strip_prefix(dir)`.
/// Only file entries will be visited, directories won't be returned.
pub fn iter_files_in_dir(path: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    let path = path.as_ref();
    fs::read_dir(path)
        .inspect_err(|e| {
            #[cfg(debug_assertions)]
            error!(
                "[D381] Failed to get iterator over files in dir: '{}' with err: '{e}'.",
                path.display()
            );
        })
        .map(|iter| {
            iter.filter_map(|entry| {
                if let Ok(entry) = entry
                    && entry.file_type().is_ok_and(|ty| ty.is_file())
                {
                    Some(entry.path())
                } else {
                    None
                }
            })
        })
        .into_iter()
        .flatten()
}

/// Wrapper over PathBuf with extra helpers for files.
///
/// When the FilePath instance is created, the path is known
/// to exist and be a file, but the file may be moved or
/// deleted at any point so it can't be guaranteed.
#[derive(Clone, Debug, Deref)]
pub struct FilePath(PathBuf);

impl FilePath {
    /// Construct a new FilePath, returning `None` if the provided
    /// path does not exist or is a directory.
    pub fn new(path: PathBuf) -> Option<Self> {
        if path.metadata().is_ok_and(|meta| meta.is_file()) {
            Some(Self(path))
        } else {
            None
        }
    }

    /// Read the file data into a Vec<u8>, returning an
    /// empty Vec<u8> if the read fails.
    pub fn read(&self) -> Vec<u8> {
        self.try_read().unwrap_or_default()
    }

    /// Try to read the file data to a Vec<u8>,
    /// returning a None if the read fails.
    pub fn try_read(&self) -> Option<Vec<u8>> {
        fs::read(&self.0).ok()
    }

    /// Read and deserialize the file's data to T.
    /// The deserializer is chosen based on the file's extension.
    /// Supported extensions:
    ///  - ron (via bevy::asset::ron)
    ///  - toml (via the toml crate)
    pub fn parse<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        let data = self.try_read()?;
        match self.extension()? {
            "ron" => bevy::asset::ron::de::from_bytes(&data).ok(),
            "toml" => toml::from_slice(&data).ok(),
            _ => None,
        }
    }

    /// Write data to the File at this path.
    ///
    /// This function will create the file if it doesn't exist, and
    /// overwrite any existing contents if it does.
    ///
    /// If an error occurs while writing, it will be logged.
    pub fn write<C: AsRef<[u8]>>(&self, contents: C) {
        if let Err(e) = fs::write(&self.0, contents.as_ref()) {
            error!(
                "[D144] Failed to write to file at path: '{}' with error: '{e}'",
                self.0.display()
            )
        }
    }

    /// Append bytes to this file, creating if it doesn't exist.
    ///
    /// If opening the file fails or appending fails,
    /// the error will be logged.
    pub fn append<C: AsRef<[u8]>>(&self, contents: C) {
        match fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.0)
        {
            Err(e) => {
                error!(
                    "[D145] Failed to open to file in append mode at path: '{}' with error: '{e}'",
                    self.0.display()
                )
            }
            Ok(mut file) => {
                if let Err(e) = file.write_all(contents.as_ref()) {
                    error!(
                        "[D146] Failed to append contents to file at path '{}' with error: '{e}'",
                        self.0.display()
                    )
                }
            }
        }
    }

    /// Delete the file at this path.
    pub fn delete(self) {
        if let Err(e) = fs::remove_file(&self.0) {
            error!(
                "[D147] Failed to remove file at path: '{}' with error: '{e}'",
                self.0.display()
            )
        }
    }

    /// Get a UTF-8 compliant name for the file without the extension.
    pub fn name(&self) -> String {
        if let Some(os_str) = self.0.file_name() {
            if let Some(os_str) = Path::new(os_str).file_stem() {
                return os_str.to_string_lossy().to_string();
            }
        }

        "UNKOWN".into()
    }

    /// Get the file extension.
    pub fn extension(&self) -> Option<&str> {
        self.0.extension().and_then(|ext| ext.to_str())
    }
}

impl AsRef<Path> for FilePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Wrapper over PathBuf with extra helpers for Directories.
#[derive(Clone, Debug, Deref)]
pub struct Directory(PathBuf);

impl Directory {
    /// Construct a new Directory, returning None if the path
    /// does not exist or is not a directory.
    pub fn new(path: PathBuf) -> Option<Self> {
        if path.metadata().is_ok_and(|meta| meta.is_dir()) {
            Some(Self(path))
        } else {
            None
        }
    }

    /// Get the path as a UTF-8 str.
    pub fn as_str(&self) -> &str {
        self.0.to_str().unwrap_or("UNKOWN")
    }

    /// Get the parent directory of this directory if there is one.
    /// If the path is equal to the mount root, the parent may be
    /// out-of-bounds for the mount. There's no check for that.
    pub fn parent(&self) -> Option<Directory> {
        self.0.parent().map(|parent| Self(parent.to_path_buf()))
    }

    /// Get the child directory with this name, but only if
    /// the new path is a directory and exists.
    pub fn child(&self, name: impl AsRef<Path>) -> Option<Self> {
        Self::new(self.0.join(name.as_ref()))
    }

    /// Create a child directory. Returns None if creation failed.
    /// Will log if an io error occurs.
    pub fn add_child(&self, name: impl AsRef<Path>) -> Option<Directory> {
        let path = self.0.join(name.as_ref());
        match fs::create_dir(&path) {
            Ok(_) => Some(Directory(path)),
            Err(e) => {
                error!(
                    "[D667] Failed to create child directory at path '{}' with error: '{e}'",
                    path.display()
                );
                None
            }
        }
    }

    /// Get an iterator over the files and directories in this Directory.
    pub fn entries(&self) -> impl Iterator<Item = Entry> {
        fs::read_dir(&self.0)
            .ok()
            .map(|iter| {
                iter.filter_map(|entry| {
                    if let Ok(entry) = entry {
                        if let Ok(ty) = entry.file_type() {
                            if ty.is_file() {
                                Some(Entry::File(FilePath(entry.path())))
                            } else if ty.is_dir() {
                                Some(Entry::Dir(Directory(entry.path())))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            })
            .into_iter()
            .flatten()
    }

    /// Get an iterator over the files in this directory.
    pub fn files(&self) -> impl Iterator<Item = FilePath> {
        fs::read_dir(&self.0)
            .ok()
            .map(|iter| {
                iter.filter_map(|entry| {
                    if let Ok(entry) = entry
                        && entry.file_type().is_ok_and(|ty| ty.is_file())
                    {
                        Some(FilePath(entry.path()))
                    } else {
                        None
                    }
                })
            })
            .into_iter()
            .flatten()
    }

    /// Get an iterator over the child directories of this directory.
    pub fn children(&self) -> impl Iterator<Item = Directory> {
        fs::read_dir(&self.0)
            .ok()
            .map(|iter| {
                iter.filter_map(|entry| {
                    if let Ok(entry) = entry
                        && entry.file_type().is_ok_and(|ty| ty.is_dir())
                    {
                        Some(Directory(entry.path()))
                    } else {
                        None
                    }
                })
            })
            .into_iter()
            .flatten()
    }
}

/// An entry in a Directory.
#[derive(Clone, Debug)]
pub enum Entry {
    Dir(Directory),
    File(FilePath),
}

#[derive(Debug)]
pub enum FileReadError {
    NotFound,
    Other(io::Error),
}
