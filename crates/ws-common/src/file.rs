use std::ffi::OsString;
use std::fs;
use std::io::{ErrorKind, Read, Seek, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Error;

const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

pub fn private_dir(dir: &Path) -> Result<(), Error> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, DIR_MODE);
    builder.create(dir).map_err(io(dir))
}

pub fn read(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(path)(e)),
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, Error> {
    read(path)?.filter(|bytes| !blank(bytes)).map(|bytes| parse(path, &bytes)).transpose()
}

/// Atomic: a reader sees the old contents or the new, never a mix.
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    parent(path)?;
    let mut staging = OsString::from(path.as_os_str());
    staging.push(".tmp");
    let staging = PathBuf::from(staging);
    let mut file = options().write(true).create(true).truncate(true).open(&staging).map_err(io(&staging))?;
    file.write_all(bytes).and_then(|()| file.sync_data()).map_err(io(&staging))?;
    fs::rename(&staging, path).map_err(io(path))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Error> {
    write(path, &encode(path, value)?)
}

pub fn remove(path: &Path) -> Result<bool, Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io(path)(e)),
    }
}

pub fn remove_dir(dir: &Path) -> Result<bool, Error> {
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io(dir)(e)),
    }
}

pub fn append(path: &Path) -> Result<fs::File, Error> {
    parent(path)?;
    options().append(true).create(true).open(path).map_err(io(path))
}

/// A JSON document under an exclusive lock shared between processes. A corrupt file is an error, never reset.
#[derive(Debug)]
pub struct Locked<T> {
    path: PathBuf,
    file: fs::File,
    value: T,
}

impl<T: Serialize + DeserializeOwned + Default> Locked<T> {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let file = Self::file(path)?;
        file.lock().map_err(io(path))?;
        Self::load(path, file)
    }

    pub fn try_open(path: &Path) -> Result<Option<Self>, Error> {
        let file = Self::file(path)?;
        match file.try_lock() {
            Ok(()) => Self::load(path, file).map(Some),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(e)) => Err(io(path)(e)),
        }
    }

    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

/// Rewrites in place, not by rename, because the lock is held on this file.
    pub fn save(&mut self) -> Result<(), Error> {
        let bytes = encode(&self.path, &self.value)?;
        let path = &self.path;
        self.file.set_len(0).map_err(io(path))?;
        self.file.rewind().map_err(io(path))?;
        self.file.write_all(&bytes).and_then(|()| self.file.sync_data()).map_err(io(path))
    }

    fn file(path: &Path) -> Result<fs::File, Error> {
        parent(path)?;
        options().read(true).write(true).create(true).truncate(false).open(path).map_err(io(path))
    }

    fn load(path: &Path, mut file: fs::File) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(io(path))?;
        let value = if blank(&bytes) { T::default() } else { parse(path, &bytes)? };
        Ok(Self { path: path.to_owned(), file, value })
    }
}

fn options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, FILE_MODE);
    options
}

fn parent(path: &Path) -> Result<(), Error> {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => private_dir(dir),
        _ => Ok(()),
    }
}

fn blank(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_whitespace)
}

fn parse<T: DeserializeOwned>(path: &Path, bytes: &[u8]) -> Result<T, Error> {
    serde_json::from_slice(bytes).map_err(|source| Error::Corrupt { path: path.to_owned(), source })
}

fn encode<T: Serialize>(path: &Path, value: &T) -> Result<Vec<u8>, Error> {
    serde_json::to_vec_pretty(value).map_err(|source| Error::Corrupt { path: path.to_owned(), source })
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> Error + '_ {
    move |source| Error::Io { path: path.to_owned(), source }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::fixtures::Scratch;

    #[cfg(unix)]
    fn mode(path: &Path) -> Option<u32> {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).map(|m| m.permissions().mode() & 0o777).ok()
    }

    #[test]
    fn writes_are_private_atomic_and_readable_back() {
        let scratch = Scratch::new();
        let path = scratch.path("nested/value.json");
        assert_eq!(read_json::<u32>(&path).ok().flatten(), None);
        write_json(&path, &7_u32).unwrap_or_else(|e| panic!("{e}"));
        write_json(&path, &8_u32).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(read_json::<u32>(&path).ok().flatten(), Some(8));
        assert!(!scratch.path("nested/value.json.tmp").exists());
        #[cfg(unix)]
        {
            assert_eq!(mode(&path), Some(FILE_MODE));
            assert_eq!(path.parent().and_then(mode), Some(DIR_MODE));
        }
        assert_eq!(remove(&path).ok(), Some(true));
        assert_eq!(remove(&path).ok(), Some(false));
    }

    #[test]
    fn appends_keep_earlier_lines() {
        let scratch = Scratch::new();
        let path = scratch.path("log.jsonl");
        for line in ["a\n", "b\n"] {
            append(&path).and_then(|mut f| f.write_all(line.as_bytes()).map_err(io(&path))).unwrap_or_else(|e| panic!("{e}"));
        }
        assert_eq!(fs::read_to_string(&path).ok().as_deref(), Some("a\nb\n"));
        #[cfg(unix)]
        assert_eq!(mode(&path), Some(FILE_MODE));
    }

    #[test]
    fn locked_documents_persist_and_exclude_other_holders() {
        let scratch = Scratch::new();
        let path = scratch.path("doc.json");
        {
            let mut doc = Locked::<BTreeMap<String, u32>>::open(&path).unwrap_or_else(|e| panic!("{e}"));
            assert!(doc.value().is_empty());
            doc.value_mut().insert("orders".into(), 2);
            doc.save().unwrap_or_else(|e| panic!("{e}"));
            assert!(matches!(Locked::<BTreeMap<String, u32>>::try_open(&path), Ok(None)));
        }
        let doc = Locked::<BTreeMap<String, u32>>::try_open(&path).ok().flatten();
        assert_eq!(doc.map(|d| d.value().get("orders").copied()), Some(Some(2)));
    }

    #[test]
    fn a_document_locked_but_never_saved_reads_as_missing() {
        let scratch = Scratch::new();
        let path = scratch.path("doc.json");
        drop(Locked::<BTreeMap<String, u32>>::open(&path).unwrap_or_else(|e| panic!("{e}")));
        assert!(matches!(read_json::<BTreeMap<String, u32>>(&path), Ok(None)));
    }

    #[test]
    fn corrupt_documents_fail_closed() {
        let scratch = Scratch::new();
        let path = scratch.path("doc.json");
        write(&path, b"{oops").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(Locked::<BTreeMap<String, u32>>::open(&path), Err(Error::Corrupt { .. })));
        assert!(matches!(read_json::<u32>(&path), Err(Error::Corrupt { .. })));
    }
}
