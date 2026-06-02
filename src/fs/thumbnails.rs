use image::GenericImageView;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ThumbnailKey {
    path: PathBuf,
    modified_secs: u64,
    size_px: u32,
    freedesktop_size: FreedesktopThumbnailSize,
    freedesktop_cache_filename: Option<String>,
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
    pub(crate) cache_paths: Option<FreedesktopThumbnailCachePaths>,
    pub(crate) data: io::Result<ThumbnailData>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub(crate) enum FreedesktopThumbnailSize {
    Normal = 128,
    Large = 256,
    XLarge = 512,
    XXLarge = 1024,
}

impl FreedesktopThumbnailSize {
    pub(crate) fn from_pixel_size(pixel_size: u32) -> Self {
        if pixel_size <= Self::Normal.pixel_size() {
            Self::Normal
        } else if pixel_size <= Self::Large.pixel_size() {
            Self::Large
        } else if pixel_size <= Self::XLarge.pixel_size() {
            Self::XLarge
        } else {
            Self::XXLarge
        }
    }

    pub(crate) const fn pixel_size(self) -> u32 {
        self as u32
    }

    pub(crate) const fn subdirectory_name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Large => "large",
            Self::XLarge => "x-large",
            Self::XXLarge => "xx-large",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FreedesktopThumbnailCachePaths {
    pub(crate) source_uri: String,
    pub(crate) cache_filename: String,
    pub(crate) size: FreedesktopThumbnailSize,
    pub(crate) thumbnail_path: PathBuf,
    pub(crate) fail_marker_path: PathBuf,
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
    let metadata = fs::metadata(path)?;
    let modified_secs = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let source_uri = thumbnail_file_uri(path)?;

    Ok(ThumbnailKey {
        path: path.to_path_buf(),
        modified_secs,
        size_px,
        freedesktop_size: FreedesktopThumbnailSize::from_pixel_size(size_px),
        freedesktop_cache_filename: Some(thumbnail_cache_filename(&source_uri)),
    })
}

pub(crate) fn fallback_key(path: &Path, size_px: u32) -> ThumbnailKey {
    ThumbnailKey {
        path: path.to_path_buf(),
        modified_secs: 0,
        size_px,
        freedesktop_size: FreedesktopThumbnailSize::from_pixel_size(size_px),
        freedesktop_cache_filename: None,
    }
}

pub(crate) fn load_thumbnail(path: PathBuf, size_px: u32) -> ThumbnailLoad {
    let key = key_for(&path, size_px).unwrap_or_else(|_| fallback_key(&path, size_px));
    let cache_paths = freedesktop_thumbnail_cache_paths(&path, size_px).unwrap_or_default();
    let data = decode_thumbnail(&path, size_px);
    ThumbnailLoad {
        path,
        key,
        cache_paths,
        data,
    }
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

pub(crate) fn freedesktop_thumbnail_cache_base_dir() -> Option<PathBuf> {
    let xdg_cache_home = env::var_os("XDG_CACHE_HOME").map(PathBuf::from);
    let home = env::var_os("HOME").map(PathBuf::from);
    thumbnail_cache_base_dir_from_values(xdg_cache_home.as_deref(), home.as_deref())
}

pub(crate) fn freedesktop_thumbnail_cache_paths(
    path: &Path,
    size_px: u32,
) -> io::Result<Option<FreedesktopThumbnailCachePaths>> {
    let Some(cache_base_dir) = freedesktop_thumbnail_cache_base_dir() else {
        return Ok(None);
    };
    Ok(Some(freedesktop_thumbnail_cache_paths_for_base(
        &thumbnail_file_uri(path)?,
        size_px,
        &cache_base_dir,
    )))
}

fn thumbnail_cache_base_dir_from_values(
    xdg_cache_home: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    xdg_cache_home
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.join("thumbnails"))
        .or_else(|| home.map(|path| path.join(".cache").join("thumbnails")))
}

fn freedesktop_thumbnail_cache_paths_for_base(
    source_uri: &str,
    size_px: u32,
    cache_base_dir: &Path,
) -> FreedesktopThumbnailCachePaths {
    let size = FreedesktopThumbnailSize::from_pixel_size(size_px);
    let cache_filename = thumbnail_cache_filename(source_uri);
    FreedesktopThumbnailCachePaths {
        source_uri: source_uri.to_string(),
        thumbnail_path: cache_base_dir
            .join(size.subdirectory_name())
            .join(&cache_filename),
        fail_marker_path: cache_base_dir
            .join("fail")
            .join(freedesktop_fail_marker_app_id())
            .join(&cache_filename),
        cache_filename,
        size,
    }
}

fn thumbnail_file_uri(path: &Path) -> io::Result<String> {
    let absolute_path = fs::canonicalize(path)?;
    file_uri_from_absolute_path(&absolute_path)
}

fn file_uri_from_absolute_path(path: &Path) -> io::Result<String> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("thumbnail path is not absolute: {}", path.display()),
        ));
    }

    let mut uri = String::from("file://");
    uri.push_str(&percent_encode_path(path));
    Ok(uri)
}

fn percent_encode_path(path: &Path) -> String {
    let mut encoded = String::new();
    for byte in path_bytes(path) {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push('%');
                encoded.push(hex_digit_upper(byte >> 4));
                encoded.push(hex_digit_upper(byte & 0x0f));
            }
        }
    }
    encoded
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

fn thumbnail_cache_filename(source_uri: &str) -> String {
    format!("{}.png", md5_hex(source_uri.as_bytes()))
}

fn freedesktop_fail_marker_app_id() -> String {
    format!("fika-{}", env!("CARGO_PKG_VERSION"))
}

fn md5_hex(input: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76a_a478,
        0xe8c7_b756,
        0x2420_70db,
        0xc1bd_ceee,
        0xf57c_0faf,
        0x4787_c62a,
        0xa830_4613,
        0xfd46_9501,
        0x6980_98d8,
        0x8b44_f7af,
        0xffff_5bb1,
        0x895c_d7be,
        0x6b90_1122,
        0xfd98_7193,
        0xa679_438e,
        0x49b4_0821,
        0xf61e_2562,
        0xc040_b340,
        0x265e_5a51,
        0xe9b6_c7aa,
        0xd62f_105d,
        0x0244_1453,
        0xd8a1_e681,
        0xe7d3_fbc8,
        0x21e1_cde6,
        0xc337_07d6,
        0xf4d5_0d87,
        0x455a_14ed,
        0xa9e3_e905,
        0xfcef_a3f8,
        0x676f_02d9,
        0x8d2a_4c8a,
        0xfffa_3942,
        0x8771_f681,
        0x6d9d_6122,
        0xfde5_380c,
        0xa4be_ea44,
        0x4bde_cfa9,
        0xf6bb_4b60,
        0xbebf_bc70,
        0x289b_7ec6,
        0xeaa1_27fa,
        0xd4ef_3085,
        0x0488_1d05,
        0xd9d4_d039,
        0xe6db_99e5,
        0x1fa2_7cf8,
        0xc4ac_5665,
        0xf429_2244,
        0x432a_ff97,
        0xab94_23a7,
        0xfc93_a039,
        0x655b_59c3,
        0x8f0c_cc92,
        0xffef_f47d,
        0x8584_5dd1,
        0x6fa8_7e4f,
        0xfe2c_e6e0,
        0xa301_4314,
        0x4e08_11a1,
        0xf753_7e82,
        0xbd3a_f235,
        0x2ad7_d2bb,
        0xeb86_d391,
    ];

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0 = 0x6745_2301u32;
    let mut b0 = 0xefcd_ab89u32;
    let mut c0 = 0x98ba_dcfeu32;
    let mut d0 = 0x1032_5476u32;

    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 16];
        for (index, word) in words.iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_le_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }

        let mut a = a0;
        let mut b = b0;
        let mut c = c0;
        let mut d = d0;

        for i in 0..64 {
            let (f, g) = if i < 16 {
                ((b & c) | ((!b) & d), i)
            } else if i < 32 {
                ((d & b) | ((!d) & c), (5 * i + 1) % 16)
            } else if i < 48 {
                (b ^ c ^ d, (3 * i + 5) % 16)
            } else {
                (c ^ (b | (!d)), (7 * i) % 16)
            };
            let next = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(words[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(next.rotate_left(S[i]));
        }

        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut digest = [0u8; 16];
    digest[0..4].copy_from_slice(&a0.to_le_bytes());
    digest[4..8].copy_from_slice(&b0.to_le_bytes());
    digest[8..12].copy_from_slice(&c0.to_le_bytes());
    digest[12..16].copy_from_slice(&d0.to_le_bytes());
    hex_lower(&digest)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(hex_digit(byte >> 4));
        output.push(hex_digit(byte & 0x0f));
    }
    output
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + nibble - 10) as char,
        _ => unreachable!("hex nibble out of range"),
    }
}

fn hex_digit_upper(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'A' + nibble - 10) as char,
        _ => unreachable!("hex nibble out of range"),
    }
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

    #[test]
    fn md5_hex_matches_standard_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(b"file:///tmp/photo.png"),
            "349e8bd0c92f85744670cd358ee23caa"
        );
    }

    #[test]
    fn freedesktop_thumbnail_size_buckets_match_spec_directories() {
        assert_eq!(
            FreedesktopThumbnailSize::from_pixel_size(64),
            FreedesktopThumbnailSize::Normal
        );
        assert_eq!(
            FreedesktopThumbnailSize::from_pixel_size(256),
            FreedesktopThumbnailSize::Large
        );
        assert_eq!(
            FreedesktopThumbnailSize::from_pixel_size(300),
            FreedesktopThumbnailSize::XLarge
        );
        assert_eq!(
            FreedesktopThumbnailSize::from_pixel_size(900),
            FreedesktopThumbnailSize::XXLarge
        );
        assert_eq!(
            FreedesktopThumbnailSize::XLarge.subdirectory_name(),
            "x-large"
        );
    }

    #[test]
    fn file_uri_percent_encodes_path_bytes_like_freedesktop_cache() {
        let uri = file_uri_from_absolute_path(Path::new("/tmp/a b[c].png")).unwrap();
        assert_eq!(uri, "file:///tmp/a%20b%5Bc%5D.png");
        assert_eq!(
            thumbnail_cache_filename(&uri),
            "82c4004aa537da39703b27ea9d450cca.png"
        );
    }

    #[test]
    fn freedesktop_cache_paths_follow_thumbnail_spec() {
        let paths = freedesktop_thumbnail_cache_paths_for_base(
            "file:///tmp/photo.png",
            96,
            Path::new("/cache/thumbnails"),
        );

        assert_eq!(paths.source_uri, "file:///tmp/photo.png");
        assert_eq!(paths.cache_filename, "349e8bd0c92f85744670cd358ee23caa.png");
        assert_eq!(paths.size, FreedesktopThumbnailSize::Normal);
        assert_eq!(
            paths.thumbnail_path,
            Path::new("/cache/thumbnails/normal/349e8bd0c92f85744670cd358ee23caa.png")
        );
        assert_eq!(
            paths.fail_marker_path,
            Path::new("/cache/thumbnails/fail/fika-0.1.0/349e8bd0c92f85744670cd358ee23caa.png")
        );
    }

    #[test]
    fn cache_base_dir_prefers_xdg_cache_home_and_falls_back_to_home() {
        assert_eq!(
            thumbnail_cache_base_dir_from_values(
                Some(Path::new("/xdg-cache")),
                Some(Path::new("/home/user"))
            ),
            Some(PathBuf::from("/xdg-cache/thumbnails"))
        );
        assert_eq!(
            thumbnail_cache_base_dir_from_values(
                Some(Path::new("")),
                Some(Path::new("/home/user"))
            ),
            Some(PathBuf::from("/home/user/.cache/thumbnails"))
        );
        assert_eq!(thumbnail_cache_base_dir_from_values(None, None), None);
    }
}
