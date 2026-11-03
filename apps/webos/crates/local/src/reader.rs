//! Reading a file a stretch at a time: the parsers ask for bytes at an
//! offset, as many as there are up to what they ask, and never hold more of
//! the file than that.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

/// The most one read gives, whatever is asked: the largest the parsers ask
/// for is the Cues' 32 MiB.
pub const READ_MOST: usize = 32 * 1024 * 1024;

/// A file, or bytes standing in for one, read at offsets.
pub trait ReadAt {
    /// Up to `count` bytes from `offset`: fewer at the file's end, none past
    /// it or when the file can't be read.
    fn read_at(&self, offset: u64, count: usize) -> Vec<u8>;

    /// The file's length in bytes.
    fn size(&self) -> u64;
}

impl ReadAt for [u8] {
    fn read_at(&self, offset: u64, count: usize) -> Vec<u8> {
        let Ok(from) = usize::try_from(offset) else { return Vec::new() };
        let to = from.saturating_add(count.min(READ_MOST)).min(self.len());
        self.get(from..to).map(<[u8]>::to_vec).unwrap_or_default()
    }

    fn size(&self) -> u64 {
        self.len() as u64
    }
}

impl ReadAt for Vec<u8> {
    fn read_at(&self, offset: u64, count: usize) -> Vec<u8> {
        self.as_slice().read_at(offset, count)
    }

    fn size(&self) -> u64 {
        self.len() as u64
    }
}

/// A file on the drive, open once and read from any thread, a seek and a
/// read at a time.
pub struct FileReader {
    file: Mutex<File>,
    size: u64,
}

impl FileReader {
    /// Opens `path` for reading.
    pub fn open(path: &Path) -> io::Result<FileReader> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        Ok(FileReader { file: Mutex::new(file), size })
    }
}

impl ReadAt for FileReader {
    fn read_at(&self, offset: u64, count: usize) -> Vec<u8> {
        if offset >= self.size {
            return Vec::new();
        }
        let count = count.min(READ_MOST) as u64;
        let mut bytes = Vec::with_capacity(count.min(self.size - offset) as usize);
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return Vec::new();
        }
        // A drive pulled out mid-read gives what came before it.
        let _ = (&mut *file).take(count).read_to_end(&mut bytes);
        bytes
    }

    fn size(&self) -> u64 {
        self.size
    }
}
