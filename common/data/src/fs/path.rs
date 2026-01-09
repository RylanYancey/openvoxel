use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use bevy::{
    ecs::intern::Interner,
    log::{debug, error, info, warn},
    prelude::Deref,
    tasks::{IoTaskPool, Task},
};
use crossbeam_channel::Receiver;
use protocol::bytes::Bytes;
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

#[derive(Debug)]
pub enum FileLoadError {
    NotFound,
    Other(io::Error),
}

#[derive(Clone, Debug)]
pub struct FilePath(PathBuf);

impl FilePath {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Load data from a file, blocking until the load is complete.
    pub fn load(&self) -> Result<Bytes, FileLoadError> {
        match fs::read(&self) {
            Ok(data) => Ok(Bytes::from(data)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(FileLoadError::NotFound),
            Err(e) => Err(FileLoadError::Other(e)),
        }
    }

    /// Save data to a file, blocking until the write is complete.
    pub fn save<C: AsRef<[u8]>>(&self, contents: C) {
        match fs::write(self, contents) {
            Ok(_) => debug!("Data written to file '{}' successfully.", self.0.display()),
            Err(e) => {
                warn!(
                    "Failed to write to file '{}' with error: '{e:?}'",
                    self.0.display(),
                )
            }
        }
    }

    /// Replace the file's extension.
    pub fn set_ext(&mut self, ext: FileExt) {
        self.0.set_extension(ext.as_str());
    }

    /// Returns a path with the provided extension instead of the existing one.
    pub fn with_ext(self, ext: FileExt) -> Self {
        Self(self.0.with_extension(ext.as_str()))
    }

    /// Get this file's extension.
    pub fn ext(&self) -> FileExt {
        FileExt::from(self)
    }

    /// Get name of the file as an &str.
    /// The returned string WILL NOT contain an extension.
    /// If the name is not valid UTF-8, or the path is empty, "" will be returned.
    pub fn name(&self) -> &str {
        self.0.file_stem().and_then(|s| s.to_str()).unwrap_or("")
    }

    /// Get the parent directory.
    pub fn parent(&self) -> &Path {
        self.0.parent().unwrap_or_else(|| Path::new(""))
    }

    /// Get the directory containing the file.
    pub fn into_parent(self) -> DirPath {
        DirPath(self.0.parent().unwrap_or_else(|| Path::new("")).into())
    }

    /// Get the path without the prefix, if it contains the prefix.
    pub fn as_relative(&self, prefix: impl AsRef<Path>) -> Option<&Path> {
        self.0.strip_prefix(prefix).ok()
    }
}

impl<P: Into<PathBuf>> From<P> for FilePath {
    fn from(value: P) -> Self {
        Self(value.into())
    }
}

impl AsRef<Path> for FilePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct DirPath(PathBuf);

impl DirPath {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn join(self, path: impl AsRef<Path>) -> Self {
        Self(self.0.join(path))
    }

    pub fn join_file(self, path: impl AsRef<Path>) -> FilePath {
        FilePath(self.0.join(path))
    }
}

impl<P: Into<PathBuf>> From<P> for DirPath {
    fn from(value: P) -> Self {
        Self(value.into())
    }
}

impl AsRef<Path> for DirPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Helper for extracting and storing file extensions.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum FileExt {
    /// The file does not have an extension.
    None,

    /// The file has an extension, but it is unsupported.
    Unsupported(&'static str),

    // Image Formats
    Png,
    Jpg,
    Jpeg,

    // Markup Formats
    Toml,
    Json,
    Ron,
}

impl FileExt {
    pub const IMAGE_FORMATS: &'static [Self] = &[Self::Png, Self::Jpeg, Self::Jpg];

    pub fn is_image_file(&self) -> bool {
        matches!(*self, Self::Png | Self::Jpg | Self::Jpeg)
    }

    pub fn is_markup_file(&self) -> bool {
        matches!(*self, Self::Ron | Self::Toml | Self::Json)
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported(_))
    }

    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Jpeg => "jpeg",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Ron => "ron",
            Self::Unsupported(s) => s,
            Self::None => "",
        }
    }
}

impl<A: AsRef<Path>> From<A> for FileExt {
    fn from(value: A) -> Self {
        match value.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some("png") => Self::Png,
            Some("jpg") => Self::Jpg,
            Some("jpeg") => Self::Jpeg,
            Some("toml") => Self::Toml,
            Some("json") => Self::Json,
            Some("ron") => Self::Ron,
            Some(s) => Self::Unsupported(FILE_EXT_INTERNER.intern(s).0),
            _ => Self::None,
        }
    }
}

impl AsRef<str> for FileExt {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

static FILE_EXT_INTERNER: Interner<str> = Interner::new();

pub struct LoadingDir {
    rx: Receiver<Result<FileData, FileLoadError>>,
}

pub struct FileData {
    pub meta: Option<Bytes>,
    pub path: FilePath,
    pub data: Bytes,
}
