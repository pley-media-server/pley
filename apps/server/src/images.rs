//! Artwork delivery: resizes images to a few width buckets (cached on disk so
//! each size is only computed once) and generates missing artwork on demand
//! through the transcoder (video frame grabs, embedded cover art).

use std::io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use dashmap::DashMap;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageReader, ImageResult, Limits};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::error::AppError;

/// Pictures come from users and the internet, and a small file can claim
/// huge dimensions: decoding stops at these limits instead of allocating.
const MAX_DIMENSION: u32 = 12_000;
const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;

/// Uploaded pictures decoded at the same time (each can take a few hundred MB).
pub static UPLOAD_DECODES: Semaphore = Semaphore::const_new(2);

fn limited<R: BufRead + Seek>(mut reader: ImageReader<R>, dimension: u32, bytes: u64) -> ImageResult<DynamicImage> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(dimension);
    limits.max_image_height = Some(dimension);
    limits.max_alloc = Some(bytes);
    reader.limits(limits);
    reader.decode()
}

/// Decode a picture (any supported format) within the size limits.
pub fn decode(bytes: &[u8]) -> ImageResult<DynamicImage> {
    limited(ImageReader::new(Cursor::new(bytes)).with_guessed_format()?, MAX_DIMENSION, MAX_DECODE_BYTES)
}

/// Decode a profile picture, which ends up 256 pixels wide: anyone signed in
/// may upload one, so it may take a fraction of what artwork may.
pub fn decode_avatar(bytes: &[u8]) -> ImageResult<DynamicImage> {
    limited(ImageReader::new(Cursor::new(bytes)).with_guessed_format()?, 4096, 96 << 20)
}

/// The EXIF tag Kometa marks a poster with once it has drawn its overlay on
/// it (`"overlay"`): that poster is never taken for an original again.
const OVERLAY_TAG: u16 = 0x04BC;
const OVERLAY: &[u8] = b"overlay";

/// Whether a picture carries Kometa's overlay mark.
pub fn has_overlay_mark(bytes: &[u8]) -> bool {
    use image::ImageDecoder;
    let exif = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_decoder().ok())
        .and_then(|mut decoder| decoder.exif_metadata().ok().flatten());
    exif.is_some_and(|chunk| exif_text(&chunk, OVERLAY_TAG).is_some_and(|text| text == OVERLAY))
}

/// An EXIF chunk with Kometa's overlay mark alone, for a picture that had it.
pub fn overlay_exif() -> Vec<u8> {
    let value = [OVERLAY, b"\0"].concat();
    let mut chunk = Vec::with_capacity(26 + value.len());
    chunk.extend_from_slice(b"II*\0");
    chunk.extend_from_slice(&8u32.to_le_bytes()); // the first directory, right after
    chunk.extend_from_slice(&1u16.to_le_bytes()); // of one entry:
    chunk.extend_from_slice(&OVERLAY_TAG.to_le_bytes());
    chunk.extend_from_slice(&2u16.to_le_bytes()); // text
    chunk.extend_from_slice(&(value.len() as u32).to_le_bytes());
    chunk.extend_from_slice(&26u32.to_le_bytes()); // after the directory
    chunk.extend_from_slice(&0u32.to_le_bytes()); // and no other directory
    chunk.extend_from_slice(&value);
    chunk
}

/// The text (or bytes) of `tag` in the first directory of an EXIF chunk,
/// without its trailing NULs.
fn exif_text(chunk: &[u8], tag: u16) -> Option<&[u8]> {
    let chunk = chunk.strip_prefix(b"Exif\0\0").unwrap_or(chunk);
    let little = match chunk.get(..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let b: [u8; 2] = chunk.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(if little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };
    let u32_at = |at: usize| -> Option<usize> {
        let b: [u8; 4] = chunk.get(at..at.checked_add(4)?)?.try_into().ok()?;
        usize::try_from(if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }).ok()
    };
    let directory = u32_at(4)?;
    for n in 0..usize::from(u16_at(directory)?) {
        let entry = directory.checked_add(2 + n * 12)?;
        if u16_at(entry)? != tag {
            continue;
        }
        // One byte a value: bytes, text or undefined.
        if !matches!(u16_at(entry + 2)?, 1 | 2 | 7) {
            return None;
        }
        let count = u32_at(entry + 4)?;
        let at = if count <= 4 { entry + 8 } else { u32_at(entry + 8)? };
        let value = chunk.get(at..at.checked_add(count)?)?;
        let end = value.iter().rposition(|b| *b != 0).map_or(0, |last| last + 1);
        return Some(&value[..end]);
    }
    None
}

const BUCKETS: [u32; 8] = [160, 240, 320, 480, 640, 960, 1280, 1920];
const JPEG_QUALITY: u8 = 82;

pub struct Images {
    cache_dir: PathBuf,
    limiter: Semaphore,
    /// Per-item locks so concurrent requests don't generate the same artwork twice.
    generating: DashMap<i64, Arc<tokio::sync::Mutex<()>>>,
    /// Frame grabs that failed lately, by file and version: not run again
    /// on every request for the picture.
    failed_grabs: DashMap<(i64, String), Instant>,
}

/// How long a failed frame grab is not tried again.
const GRAB_RETRY: Duration = Duration::from_secs(60 * 60);

pub struct ImageBytes {
    pub bytes: Bytes,
    pub content_type: &'static str,
    pub etag: String,
}

impl Images {
    pub fn new(cache_dir: PathBuf) -> Self {
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
        Self { cache_dir, limiter: Semaphore::new(workers), generating: DashMap::new(), failed_grabs: DashMap::new() }
    }

    /// Whether the frame grab of this file version failed within the hour.
    pub fn grab_failed_lately(&self, file_id: i64, fingerprint: &str) -> bool {
        self.failed_grabs.get(&(file_id, fingerprint.to_owned())).is_some_and(|at| at.elapsed() < GRAB_RETRY)
    }

    pub fn grab_failed(&self, file_id: i64, fingerprint: &str) {
        if self.failed_grabs.len() > 10_000 {
            self.failed_grabs.retain(|_, at| at.elapsed() < GRAB_RETRY);
        }
        self.failed_grabs.insert((file_id, fingerprint.to_owned()), Instant::now());
    }

    pub fn generation_lock(&self, item_id: i64) -> Arc<tokio::sync::Mutex<()>> {
        self.generating.entry(item_id).or_default().clone()
    }

    pub fn release_generation_lock(&self, item_id: i64) {
        self.generating.remove_if(&item_id, |_, lock| Arc::strong_count(lock) <= 1);
    }

    /// Serve `source`, resized to the bucket at or above `width` when smaller
    /// than the original. Read from the file opened and checked
    /// ([`crate::library::walk::open_of_its_kind`]), not by its name again:
    /// a library's `poster.jpg` swapped for a link meanwhile hands out no
    /// other file.
    pub async fn load(&self, source: &Path, width: Option<u32>) -> Result<ImageBytes, AppError> {
        let named = source.to_path_buf();
        let (file, meta) = tokio::task::spawn_blocking(move || crate::library::walk::open_of_its_kind(&named))
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(|| AppError::not_found("image not found"))?;
        if meta.len() > MAX_PICTURE_BYTES {
            return Err(AppError::not_found("image not found"));
        }
        let mtime = meta.modified().ok().map(crate::util::system_time_secs).unwrap_or(0);
        let bucket = width.map(|w| *BUCKETS.iter().find(|b| **b >= w).unwrap_or(&BUCKETS[BUCKETS.len() - 1]));
        let mut hasher = Sha256::new();
        hasher.update(source.to_string_lossy().as_bytes());
        hasher.update(meta.len().to_le_bytes());
        hasher.update(mtime.to_le_bytes());
        hasher.update(bucket.unwrap_or(0).to_le_bytes());
        let key = hex::encode(&hasher.finalize()[..16]);
        let etag = format!("\"{key}\"");

        let Some(bucket) = bucket else {
            let bytes = crate::util::blocking(move || Ok(whole(file)?)).await.map_err(AppError::Internal)?;
            return Ok(ImageBytes { content_type: content_type_of(source, &bytes), bytes: bytes.into(), etag });
        };

        // A see-through picture (a channel's logo) stays see-through: PNG.
        let dir = self.cache_dir.join(&key[..2]);
        for (ext, content_type) in [("jpg", "image/jpeg"), ("png", "image/png")] {
            if let Ok(bytes) = tokio::fs::read(dir.join(format!("{key}.{ext}"))).await {
                return Ok(ImageBytes { bytes: bytes.into(), content_type, etag });
            }
        }

        let _permit = self.limiter.acquire().await.map_err(|e| AppError::Internal(e.into()))?;
        let out = dir.join(&key);
        let (bytes, made) =
            crate::util::blocking(move || resized_or_whole(file, bucket, &out)).await.map_err(AppError::Internal)?;
        // Already small enough (or not decodable): the original.
        let content_type = made.unwrap_or_else(|| content_type_of(source, &bytes));
        Ok(ImageBytes { bytes: bytes.into(), content_type, etag })
    }
}

/// The largest picture file read here (artwork is a few MB): a file of a
/// library named like a picture is read into memory whole only up to this.
const MAX_PICTURE_BYTES: u64 = 64 << 20;

/// All of `file`, from its start, up to [`MAX_PICTURE_BYTES`].
fn whole(mut file: std::fs::File) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(MAX_PICTURE_BYTES).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// `file` resized to `width` and kept beside `out` ([`resize_to_file`]), or
/// all of it as it is when it isn't wider or can't be decoded: its bytes,
/// and their type when they were made here.
fn resized_or_whole(
    mut file: std::fs::File,
    width: u32,
    out: &Path,
) -> anyhow::Result<(Vec<u8>, Option<&'static str>)> {
    if let Ok(Some((bytes, content_type))) = resize_to_file(&mut file, width, out) {
        return Ok((bytes, Some(content_type)));
    }
    Ok((whole(file)?, None))
}

/// Resize `file`, a picture, to `width` (keeping aspect ratio) and write it
/// beside `out` (its extension added): a JPEG, or a PNG when the picture is
/// see-through. Returns `None` when the picture is not wider than the target.
fn resize_to_file(file: &mut std::fs::File, width: u32, out: &Path) -> anyhow::Result<Option<(Vec<u8>, &'static str)>> {
    let (w, h) = ImageReader::new(BufReader::new(&mut *file)).with_guessed_format()?.into_dimensions()?;
    if w <= width {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(0))?;
    let img =
        limited(ImageReader::new(BufReader::new(&mut *file)).with_guessed_format()?, MAX_DIMENSION, MAX_DECODE_BYTES)?;
    let height = ((h as f64) * (width as f64) / (w as f64)).round().max(1.0) as u32;
    let resized = img.resize_exact(width, height, image::imageops::FilterType::Triangle);
    let mut buf = Vec::with_capacity((width * height / 4) as usize);
    let (ext, content_type) = if see_through(&resized) {
        resized.to_rgba8().write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;
        ("png", "image/png")
    } else {
        JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY).encode_image(&resized.to_rgb8())?;
        ("jpg", "image/jpeg")
    };
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let path = out.with_extension(ext);
    let tmp = out.with_extension("tmp");
    std::fs::write(&tmp, &buf)?;
    std::fs::rename(&tmp, path)?;
    Ok(Some((buf, content_type)))
}

/// Whether any of a picture is see-through.
pub fn see_through(img: &DynamicImage) -> bool {
    img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p[3] < 255)
}

pub fn content_type_of(path: &Path, bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return "image/png";
    }
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return "image/webp";
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return "image/jpeg";
    }
    match path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/jpeg",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resizes_and_caches() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("poster.png");
        image::RgbImage::from_pixel(1000, 1500, image::Rgb([200, 100, 50])).save(&src).unwrap();
        let images = Images::new(dir.path().join("cache"));

        let small = images.load(&src, Some(300)).await.unwrap();
        assert_eq!(small.content_type, "image/jpeg");
        let decoded = image::load_from_memory(&small.bytes).unwrap();
        assert_eq!(decoded.width(), 320);
        assert_eq!(decoded.height(), 480);

        // Second call is served from the cache with the same ETag.
        let again = images.load(&src, Some(300)).await.unwrap();
        assert_eq!(again.etag, small.etag);

        // Wider than the source: original bytes.
        let original = images.load(&src, Some(1920)).await.unwrap();
        assert_eq!(original.content_type, "image/png");
    }

    #[tokio::test]
    async fn see_through_pictures_stay_see_through() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("logo.png");
        let mut logo = image::RgbaImage::from_pixel(800, 400, image::Rgba([0, 0, 0, 0]));
        for x in 200..600 {
            logo.put_pixel(x, 200, image::Rgba([10, 10, 10, 255]));
        }
        logo.save(&src).unwrap();
        let images = Images::new(dir.path().join("cache"));
        for _ in 0..2 {
            let small = images.load(&src, Some(240)).await.unwrap();
            assert_eq!(small.content_type, "image/png");
            let decoded = image::load_from_memory(&small.bytes).unwrap().to_rgba8();
            assert_eq!((decoded.width(), decoded[(0, 0)][3]), (240, 0), "no black box behind it");
        }
    }

    fn jpeg(exif: Option<Vec<u8>>) -> Vec<u8> {
        use image::ImageEncoder;
        let mut out = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut out, 80);
        if let Some(exif) = exif {
            encoder.set_exif_metadata(exif).unwrap();
        }
        encoder.encode_image(&image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))).unwrap();
        out
    }

    #[test]
    fn kometas_overlay_mark_is_read() {
        assert!(has_overlay_mark(&jpeg(Some(overlay_exif()))));
        assert!(!has_overlay_mark(&jpeg(None)));
        // Big-endian, as other writers make it, and another tag.
        let mut chunk = b"MM\0*\0\0\0\x08\0\x01\x04\xBC\0\x02\0\0\0\x04ovl\0\0\0\0\0".to_vec();
        assert_eq!(exif_text(&chunk, OVERLAY_TAG), Some(&b"ovl"[..]));
        chunk[11] = 0xBD;
        assert_eq!(exif_text(&chunk, OVERLAY_TAG), None);
        assert_eq!(exif_text(b"II*\0\xff\xff\xff\xff", OVERLAY_TAG), None, "a directory past the end");
    }
}
