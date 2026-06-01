use image::GenericImageView;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ThumbnailKey {
    path: PathBuf,
    modified_secs: u64,
    size_px: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct ThumbnailData {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct ThumbnailLoad {
    pub(crate) path: PathBuf,
    pub(crate) key: ThumbnailKey,
    pub(crate) data: io::Result<ThumbnailData>,
}

pub(crate) fn is_thumbnail_candidate(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "webp"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn key_for(path: &Path, size_px: u32) -> io::Result<ThumbnailKey> {
    let modified_secs = fs::metadata(path)?
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or_default();

    Ok(ThumbnailKey {
        path: path.to_path_buf(),
        modified_secs,
        size_px,
    })
}

pub(crate) fn fallback_key(path: &Path, size_px: u32) -> ThumbnailKey {
    ThumbnailKey {
        path: path.to_path_buf(),
        modified_secs: 0,
        size_px,
    }
}

pub(crate) fn load_thumbnail(path: PathBuf, size_px: u32) -> ThumbnailLoad {
    let key = key_for(&path, size_px).unwrap_or_else(|_| fallback_key(&path, size_px));
    let data = decode_thumbnail(&path, size_px);
    ThumbnailLoad { path, key, data }
}

fn decode_thumbnail(path: &Path, size_px: u32) -> io::Result<ThumbnailData> {
    let image = image::open(path).map_err(io::Error::other)?;
    let (width, height) = image.dimensions();
    let scale = (size_px as f32 / width.max(height).max(1) as f32).min(1.0);
    let target_width = ((width as f32 * scale).round() as u32).max(1);
    let target_height = ((height as f32 * scale).round() as u32).max(1);
    let resized = image.thumbnail(target_width, target_height).to_rgba8();

    Ok(ThumbnailData {
        width: resized.width(),
        height: resized.height(),
        rgba: resized.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_image_extensions() {
        assert!(is_thumbnail_candidate(Path::new("photo.JPG")));
        assert!(is_thumbnail_candidate(Path::new("photo.webp")));
        assert!(!is_thumbnail_candidate(Path::new("notes.txt")));
    }
}
