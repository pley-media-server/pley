//! The sample media every file streams: the Android app's committed clips
//! (`scripts/make-android-fixtures.ps1`), read where they are in the
//! repository rather than copied, once each, on first use.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::http::lock;

/// The two clips every file streams.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Clip {
    Sdr1080,
    Hdr2160,
}

impl Clip {
    pub fn file(self) -> &'static str {
        match self {
            Clip::Sdr1080 => "sample-1080p.mp4",
            Clip::Hdr2160 => "sample-2160p-hdr10.mkv",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Clip::Sdr1080 => "video/mp4",
            Clip::Hdr2160 => "video/x-matroska",
        }
    }

    pub fn duration_ms(self) -> i64 {
        match self {
            Clip::Sdr1080 => 20_000,
            Clip::Hdr2160 => 20_020,
        }
    }

    /// The committed file's size, for a catalogue made where the files
    /// aren't (they are then answered 404).
    fn known_size(self) -> i64 {
        match self {
            Clip::Sdr1080 => 1_379_196,
            Clip::Hdr2160 => 2_502_426,
        }
    }
}

/// How many segments the sample HLS stream has (the 1080p clip in 4 s parts).
pub(crate) const HLS_SEGMENTS: usize = 5;
/// Each segment's length.
pub(crate) const HLS_SEGMENT_MS: i64 = 4_000;

/// Where the sample media are: `PLEY_FIXTURES_MEDIA` when set, else the
/// Android fixtures' folder in this repository.
pub(crate) fn default_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PLEY_FIXTURES_MEDIA") {
        return dir.into();
    }
    repository_dir()
}

#[cfg(not(target_arch = "arm"))]
fn repository_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../android/testing/fixtures/src/main/resources/app/pley/android/fixtures/media")
}

/// The TV never serves the fixtures; leaving the path out keeps the build
/// machine's folders out of its binary.
#[cfg(target_arch = "arm")]
fn repository_dir() -> PathBuf {
    PathBuf::from("media")
}

/// The sample media, read lazily and kept.
pub(crate) struct Media {
    dir: PathBuf,
    files: Mutex<HashMap<String, Option<Arc<[u8]>>>>,
}

impl Media {
    pub fn new(dir: PathBuf) -> Media {
        Media { dir, files: Mutex::new(HashMap::new()) }
    }

    /// The bytes of `name` ("sample-1080p.mp4", "hls/0.ts"), or `None` when
    /// there is no such file.
    pub fn bytes(&self, name: &str) -> Option<Arc<[u8]>> {
        if let Some(known) = lock(&self.files).get(name) {
            return known.clone();
        }
        let read = std::fs::read(self.dir.join(name)).ok().map(Arc::<[u8]>::from);
        lock(&self.files).insert(name.to_string(), read.clone());
        read
    }

    /// A clip's size in bytes: the file's when it is there.
    pub fn size(&self, clip: Clip) -> i64 {
        std::fs::metadata(self.dir.join(clip.file())).map(|m| m.len() as i64).unwrap_or(clip.known_size())
    }

    pub fn clip(&self, clip: Clip) -> Option<Arc<[u8]>> {
        self.bytes(clip.file())
    }

    /// Segment `n` of the sample HLS stream.
    pub fn segment(&self, n: usize) -> Option<Arc<[u8]>> {
        if n >= HLS_SEGMENTS {
            return None;
        }
        self.bytes(&format!("hls/{n}.ts"))
    }
}
