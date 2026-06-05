use image::GenericImageView;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ThumbnailKey {
    path: PathBuf,
    modified_secs: u64,
    size_px: u32,
    freedesktop_size: FreedesktopThumbnailSize,
    freedesktop_cache_filename: Option<String>,
}

impl ThumbnailKey {
    pub(crate) fn item_view_media_token(&self) -> i32 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        let hash = hasher.finish();
        (((hash ^ (hash >> 32)) as u32 & 0x3fff_ffff) | 0x4000_0000) as i32
    }
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThumbnailerEntry {
    exec: String,
    mime_types: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedThumbnailerEntry {
    exec: String,
    try_exec: Option<String>,
    mime_types: Vec<String>,
}

static THUMBNAILER_REGISTRY: OnceLock<Vec<ThumbnailerEntry>> = OnceLock::new();

pub(crate) fn is_thumbnail_candidate(path: &Path) -> bool {
    if is_builtin_thumbnail_candidate(path) {
        return true;
    }

    thumbnailer_mime_from_extension(path.extension())
        .and_then(|mime_type| thumbnailer_for_mime(thumbnailer_registry(), mime_type))
        .is_some()
}

fn is_builtin_thumbnail_candidate(path: &Path) -> bool {
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
    let cache_base_dir = freedesktop_thumbnail_cache_base_dir();
    load_thumbnail_with_cache_base(path, size_px, cache_base_dir.as_deref())
}

fn load_thumbnail_with_cache_base(
    path: PathBuf,
    size_px: u32,
    cache_base_dir: Option<&Path>,
) -> ThumbnailLoad {
    let key = key_for(&path, size_px).unwrap_or_else(|_| fallback_key(&path, size_px));
    let cache_paths = cache_base_dir
        .map(|cache_base_dir| {
            freedesktop_thumbnail_cache_paths_for_path_base(&path, size_px, cache_base_dir)
        })
        .transpose()
        .unwrap_or_default();
    let data = if key.freedesktop_cache_filename.is_some() {
        if let Some(cache_paths) = cache_paths.as_ref() {
            load_thumbnail_with_freedesktop_cache(&path, size_px, &key, cache_paths)
        } else {
            decode_thumbnail(&path, size_px)
        }
    } else {
        decode_thumbnail(&path, size_px)
    };
    ThumbnailLoad {
        path,
        key,
        cache_paths,
        data,
    }
}

fn load_thumbnail_with_freedesktop_cache(
    path: &Path,
    size_px: u32,
    key: &ThumbnailKey,
    cache_paths: &FreedesktopThumbnailCachePaths,
) -> io::Result<ThumbnailData> {
    if let Some(data) = read_thumbnail_cache(cache_paths, key.modified_secs, size_px) {
        return Ok(data);
    }

    if thumbnail_disk_entry_is_fresh(&cache_paths.fail_marker_path, key.modified_secs) {
        return Err(io::Error::other(format!(
            "thumbnail generation previously failed; fail marker {}",
            cache_paths.fail_marker_path.display()
        )));
    }

    let cache_size_px = cache_paths.size.pixel_size();
    match decode_thumbnail(path, cache_size_px) {
        Ok(cache_data) => {
            let _ = write_thumbnail_cache(cache_paths, &cache_data, key.modified_secs);
            resize_thumbnail_data(&cache_data, size_px)
        }
        Err(err) => {
            if !is_builtin_thumbnail_candidate(path)
                && let Some(thumbnailer) =
                    thumbnailer_candidate_for_path(path, thumbnailer_registry())
            {
                match run_external_thumbnailer(thumbnailer, path, cache_paths, cache_size_px) {
                    Ok(()) => {
                        if let Some(data) =
                            read_thumbnail_cache(cache_paths, key.modified_secs, size_px)
                        {
                            let _ = remove_thumbnail_fail_marker(cache_paths);
                            Ok(data)
                        } else {
                            let _ = write_thumbnail_fail_marker(cache_paths);
                            Err(io::Error::other(format!(
                                "external thumbnailer did not produce a fresh readable cache file: {}",
                                cache_paths.thumbnail_path.display()
                            )))
                        }
                    }
                    Err(thumbnailer_err) => {
                        let _ = write_thumbnail_fail_marker(cache_paths);
                        Err(io::Error::other(format!(
                            "{err}; external thumbnailer failed: {thumbnailer_err}"
                        )))
                    }
                }
            } else {
                let _ = write_thumbnail_fail_marker(cache_paths);
                Err(err)
            }
        }
    }
}

fn decode_thumbnail(path: &Path, size_px: u32) -> io::Result<ThumbnailData> {
    let image = image::open(path).map_err(io::Error::other)?;
    thumbnail_dynamic_image(image, size_px)
}

fn thumbnail_dynamic_image(image: image::DynamicImage, size_px: u32) -> io::Result<ThumbnailData> {
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

fn resize_thumbnail_data(data: &ThumbnailData, size_px: u32) -> io::Result<ThumbnailData> {
    if data.width.max(data.height) <= size_px.max(1) {
        return Ok(data.clone());
    }
    let image = image::RgbaImage::from_raw(data.width, data.height, data.rgba.clone())
        .ok_or_else(|| io::Error::other("thumbnail buffer dimensions do not match RGBA data"))?;
    thumbnail_dynamic_image(image::DynamicImage::ImageRgba8(image), size_px)
}

pub(crate) fn freedesktop_thumbnail_cache_base_dir() -> Option<PathBuf> {
    let xdg_cache_home = env::var_os("XDG_CACHE_HOME").map(PathBuf::from);
    let home = env::var_os("HOME").map(PathBuf::from);
    thumbnail_cache_base_dir_from_values(xdg_cache_home.as_deref(), home.as_deref())
}

fn freedesktop_thumbnail_cache_paths_for_path_base(
    path: &Path,
    size_px: u32,
    cache_base_dir: &Path,
) -> io::Result<FreedesktopThumbnailCachePaths> {
    Ok(freedesktop_thumbnail_cache_paths_for_base(
        &thumbnail_file_uri(path)?,
        size_px,
        cache_base_dir,
    ))
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

fn read_thumbnail_cache(
    cache_paths: &FreedesktopThumbnailCachePaths,
    source_modified_secs: u64,
    size_px: u32,
) -> Option<ThumbnailData> {
    if !thumbnail_disk_entry_is_fresh(&cache_paths.thumbnail_path, source_modified_secs) {
        return None;
    }

    if !thumbnail_cache_metadata_matches(
        &cache_paths.thumbnail_path,
        &cache_paths.source_uri,
        source_modified_secs,
    ) {
        let _ = fs::remove_file(&cache_paths.thumbnail_path);
        return None;
    }

    match decode_thumbnail(&cache_paths.thumbnail_path, size_px) {
        Ok(data) => Some(data),
        Err(_) => {
            let _ = fs::remove_file(&cache_paths.thumbnail_path);
            None
        }
    }
}

fn thumbnail_cache_metadata_matches(
    path: &Path,
    expected_uri: &str,
    source_modified_secs: u64,
) -> bool {
    let Ok(mut text_chunks) = png_text_chunks(path) else {
        return false;
    };
    thumbnail_cache_text_metadata_matches(&text_chunks, expected_uri, source_modified_secs) || {
        if png_tail_text_chunks(path, &mut text_chunks).is_err() {
            return false;
        }
        thumbnail_cache_text_metadata_matches(&text_chunks, expected_uri, source_modified_secs)
    }
}

fn thumbnail_cache_text_metadata_matches(
    text_chunks: &[(String, String)],
    expected_uri: &str,
    source_modified_secs: u64,
) -> bool {
    let expected_mtime = source_modified_secs.to_string();
    let uri_matches = text_chunks
        .iter()
        .any(|(keyword, text)| keyword == "Thumb::URI" && text == expected_uri);
    let mtime_matches = text_chunks
        .iter()
        .any(|(keyword, text)| keyword == "Thumb::MTime" && text == &expected_mtime);

    uri_matches && mtime_matches
}

fn png_text_chunks(path: &Path) -> io::Result<Vec<(String, String)>> {
    let file = fs::File::open(path)?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let reader = decoder.read_info().map_err(io::Error::other)?;
    collect_png_text_chunks(reader.info())
}

fn png_tail_text_chunks(path: &Path, text_chunks: &mut Vec<(String, String)>) -> io::Result<()> {
    let file = fs::File::open(path)?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().map_err(io::Error::other)?;
    reader.finish().map_err(io::Error::other)?;
    *text_chunks = collect_png_text_chunks(reader.info())?;
    Ok(())
}

fn collect_png_text_chunks(info: &png::Info<'_>) -> io::Result<Vec<(String, String)>> {
    let mut text_chunks = info
        .uncompressed_latin1_text
        .iter()
        .map(|chunk| Ok((chunk.keyword.clone(), chunk.text.clone())))
        .collect::<io::Result<Vec<_>>>()?;

    for chunk in &info.compressed_latin1_text {
        let mut chunk = chunk.clone();
        chunk.decompress_text().map_err(io::Error::other)?;
        text_chunks.push((
            chunk.keyword.clone(),
            chunk.get_text().map_err(io::Error::other)?,
        ));
    }
    for chunk in &info.utf8_text {
        let mut chunk = chunk.clone();
        chunk.decompress_text().map_err(io::Error::other)?;
        text_chunks.push((
            chunk.keyword.clone(),
            chunk.get_text().map_err(io::Error::other)?,
        ));
    }

    Ok(text_chunks)
}

fn write_thumbnail_cache(
    cache_paths: &FreedesktopThumbnailCachePaths,
    data: &ThumbnailData,
    source_modified_secs: u64,
) -> io::Result<()> {
    let parent = cache_paths
        .thumbnail_path
        .parent()
        .ok_or_else(|| io::Error::other("thumbnail cache path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temp_path = parent.join(format!(
        ".{}.{}.tmp",
        cache_paths.cache_filename,
        std::process::id()
    ));

    write_thumbnail_cache_png(
        &temp_path,
        data,
        &cache_paths.source_uri,
        source_modified_secs,
    )?;
    match fs::rename(&temp_path, &cache_paths.thumbnail_path) {
        Ok(()) => {
            let _ = remove_thumbnail_fail_marker(cache_paths);
            Ok(())
        }
        Err(err) => {
            let _ = fs::remove_file(&temp_path);
            Err(err)
        }
    }
}

fn write_thumbnail_cache_png(
    path: &Path,
    data: &ThumbnailData,
    source_uri: &str,
    source_modified_secs: u64,
) -> io::Result<()> {
    let file = fs::File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, data.width, data.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_text_chunk("Thumb::URI".to_string(), source_uri.to_string())
        .map_err(io::Error::other)?;
    encoder
        .add_text_chunk("Thumb::MTime".to_string(), source_modified_secs.to_string())
        .map_err(io::Error::other)?;
    let mut writer = encoder.write_header().map_err(io::Error::other)?;
    writer
        .write_image_data(&data.rgba)
        .map_err(io::Error::other)
}

fn write_thumbnail_fail_marker(cache_paths: &FreedesktopThumbnailCachePaths) -> io::Result<()> {
    let parent = cache_paths
        .fail_marker_path
        .parent()
        .ok_or_else(|| io::Error::other("thumbnail fail marker path has no parent"))?;
    fs::create_dir_all(parent)?;
    fs::write(
        &cache_paths.fail_marker_path,
        format!("{}\n", cache_paths.source_uri),
    )
}

fn remove_thumbnail_fail_marker(cache_paths: &FreedesktopThumbnailCachePaths) -> io::Result<()> {
    match fs::remove_file(&cache_paths.fail_marker_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn thumbnailer_registry() -> &'static [ThumbnailerEntry] {
    THUMBNAILER_REGISTRY.get_or_init(|| discover_thumbnailers(&thumbnailer_search_dirs()))
}

fn discover_thumbnailers(search_dirs: &[PathBuf]) -> Vec<ThumbnailerEntry> {
    let mut entries = Vec::new();
    for dir in search_dirs {
        let Ok(read_dir) = fs::read_dir(dir) else {
            continue;
        };
        let mut paths = read_dir
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("thumbnailer")
            })
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            if let Some(entry) = thumbnailer_entry_from_file(&path) {
                entries.push(entry);
            }
        }
    }
    entries
}

fn thumbnailer_entry_from_file(path: &Path) -> Option<ThumbnailerEntry> {
    let content = fs::read_to_string(path).ok()?;
    let parsed = parse_thumbnailer_entry(&content).ok()?;
    if let Some(try_exec) = parsed.try_exec.as_deref()
        && !try_exec_available(try_exec)
    {
        return None;
    }
    Some(ThumbnailerEntry {
        exec: parsed.exec,
        mime_types: parsed.mime_types,
    })
}

fn parse_thumbnailer_entry(content: &str) -> Result<ParsedThumbnailerEntry, String> {
    let sections = parse_ini_sections(content);
    let section = sections
        .get("Thumbnailer Entry")
        .ok_or_else(|| "thumbnailer has no Thumbnailer Entry section".to_string())?;
    let exec = section
        .get("Exec")
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| "thumbnailer has no Exec entry".to_string())?;
    let mime_types = section
        .get("MimeType")
        .map(|value| desktop_list(value).map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();
    if mime_types.is_empty() {
        return Err("thumbnailer has no MimeType entries".to_string());
    }
    Ok(ParsedThumbnailerEntry {
        exec,
        try_exec: section
            .get("TryExec")
            .filter(|value| !value.trim().is_empty())
            .cloned(),
        mime_types,
    })
}

fn thumbnailer_search_dirs() -> Vec<PathBuf> {
    let xdg_data_home = env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    let home = env::var_os("HOME").map(PathBuf::from);
    let xdg_data_dirs = env::var_os("XDG_DATA_DIRS").map(PathBuf::from);
    thumbnailer_search_dirs_from_values(
        xdg_data_home.as_deref(),
        home.as_deref(),
        xdg_data_dirs.as_deref(),
    )
}

fn thumbnailer_search_dirs_from_values(
    xdg_data_home: Option<&Path>,
    home: Option<&Path>,
    xdg_data_dirs: Option<&Path>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = xdg_data_home.filter(|path| !path.as_os_str().is_empty()) {
        dirs.push(path.join("thumbnailers"));
    } else if let Some(home) = home {
        dirs.push(home.join(".local").join("share").join("thumbnailers"));
    }

    let data_dirs = xdg_data_dirs
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    for dir in data_dirs.split(':').filter(|dir| !dir.is_empty()) {
        dirs.push(PathBuf::from(dir).join("thumbnailers"));
    }
    dirs
}

fn thumbnailer_candidate_for_path<'a>(
    path: &Path,
    entries: &'a [ThumbnailerEntry],
) -> Option<&'a ThumbnailerEntry> {
    let mime_type = thumbnailer_mime_from_extension(path.extension())?;
    thumbnailer_for_mime(entries, mime_type)
}

fn thumbnailer_for_mime<'a>(
    entries: &'a [ThumbnailerEntry],
    mime_type: &str,
) -> Option<&'a ThumbnailerEntry> {
    entries.iter().find(|entry| {
        entry
            .mime_types
            .iter()
            .any(|candidate| thumbnailer_mime_matches(candidate, mime_type))
    })
}

fn thumbnailer_mime_matches(candidate: &str, mime_type: &str) -> bool {
    candidate == mime_type
        || candidate
            .strip_suffix("/*")
            .is_some_and(|prefix| mime_type.starts_with(&format!("{prefix}/")))
}

fn thumbnailer_mime_from_extension(extension: Option<&std::ffi::OsStr>) -> Option<&'static str> {
    let extension = extension?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" | "svgz" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "pdf" => "application/pdf",
        _ => return None,
    })
}

fn run_external_thumbnailer(
    thumbnailer: &ThumbnailerEntry,
    source_path: &Path,
    cache_paths: &FreedesktopThumbnailCachePaths,
    size_px: u32,
) -> io::Result<()> {
    let argv = expand_thumbnailer_exec(
        &thumbnailer.exec,
        source_path,
        &cache_paths.source_uri,
        &cache_paths.thumbnail_path,
        size_px,
    )
    .map_err(io::Error::other)?;
    let Some((program, args)) = argv.split_first() else {
        return Err(io::Error::other(
            "thumbnailer Exec expanded to an empty command",
        ));
    };
    let parent = cache_paths
        .thumbnail_path
        .parent()
        .ok_or_else(|| io::Error::other("thumbnail cache path has no parent"))?;
    fs::create_dir_all(parent)?;
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "thumbnailer exited with status {status}"
        )));
    }
    if !cache_paths.thumbnail_path.exists() {
        return Err(io::Error::other(format!(
            "thumbnailer did not create {}",
            cache_paths.thumbnail_path.display()
        )));
    }
    Ok(())
}

fn expand_thumbnailer_exec(
    exec: &str,
    source_path: &Path,
    source_uri: &str,
    output_path: &Path,
    size_px: u32,
) -> Result<Vec<String>, String> {
    parse_exec_words(exec)?
        .into_iter()
        .map(|arg| expand_thumbnailer_exec_arg(&arg, source_path, source_uri, output_path, size_px))
        .collect()
}

fn expand_thumbnailer_exec_arg(
    arg: &str,
    source_path: &Path,
    source_uri: &str,
    output_path: &Path,
    size_px: u32,
) -> Result<String, String> {
    let mut expanded = String::new();
    let mut chars = arg.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            expanded.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => expanded.push('%'),
            Some('i') => expanded.push_str(&source_path.to_string_lossy()),
            Some('u') => expanded.push_str(source_uri),
            Some('o') => expanded.push_str(&output_path.to_string_lossy()),
            Some('s') => expanded.push_str(&size_px.to_string()),
            Some(code) => return Err(format!("unsupported thumbnailer Exec field code %{code}")),
            None => return Err("thumbnailer Exec ends with a bare %".to_string()),
        }
    }
    Ok(expanded)
}

fn parse_exec_words(exec: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = exec.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (None, '\'' | '"') => quote = Some(ch),
            (Some(q), c) if c == q => quote = None,
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (_, '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (_, c) => current.push(c),
        }
    }
    if quote.is_some() {
        return Err("unterminated quote in thumbnailer Exec command".to_string());
    }
    if !current.is_empty() {
        args.push(current);
    }
    if args.is_empty() {
        return Err("empty thumbnailer Exec command".to_string());
    }
    Ok(args)
}

fn try_exec_available(command: &str) -> bool {
    if command.contains('/') {
        return Path::new(command).is_file();
    }
    env::var_os("PATH")
        .map(|paths| try_exec_available_in_path(command, &paths.to_string_lossy()))
        .unwrap_or(false)
}

fn try_exec_available_in_path(command: &str, path_env: &str) -> bool {
    path_env
        .split(':')
        .filter(|path| !path.is_empty())
        .any(|path| Path::new(path).join(command).is_file())
}

fn parse_ini_sections(content: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = String::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = section.trim().to_string();
            sections.entry(current.clone()).or_default();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !current.is_empty() {
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    sections
}

fn desktop_list(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
}

fn thumbnail_disk_entry_is_fresh(path: &Path, source_modified_secs: u64) -> bool {
    file_modified_secs(path).is_some_and(|modified_secs| {
        thumbnail_disk_entry_is_fresh_for_times(modified_secs, source_modified_secs)
    })
}

fn thumbnail_disk_entry_is_fresh_for_times(
    entry_modified_secs: u64,
    source_modified_secs: u64,
) -> bool {
    entry_modified_secs >= source_modified_secs
}

fn file_modified_secs(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
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
    use image::ImageFormat;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "fika-thumbnail-{name}-{}-{now}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_rgba_png(path: &Path, rgba: &[u8], width: u32, height: u32) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        image::save_buffer_with_format(
            path,
            rgba,
            width,
            height,
            image::ColorType::Rgba8,
            ImageFormat::Png,
        )
        .unwrap();
    }

    fn write_test_cache_png(
        cache_paths: &FreedesktopThumbnailCachePaths,
        data: &ThumbnailData,
        source_uri: &str,
        source_modified_secs: u64,
    ) {
        if let Some(parent) = cache_paths.thumbnail_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        write_thumbnail_cache_png(
            &cache_paths.thumbnail_path,
            data,
            source_uri,
            source_modified_secs,
        )
        .unwrap();
    }

    fn write_test_cache_png_with_tail_metadata(
        cache_paths: &FreedesktopThumbnailCachePaths,
        data: &ThumbnailData,
        source_uri: &str,
        source_modified_secs: u64,
    ) {
        if let Some(parent) = cache_paths.thumbnail_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let file = fs::File::create(&cache_paths.thumbnail_path).unwrap();
        let writer = BufWriter::new(file);
        let mut encoder = png::Encoder::new(writer, data.width, data.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&data.rgba).unwrap();
        writer
            .write_text_chunk(&png::text_metadata::TEXtChunk::new(
                "Thumb::URI".to_string(),
                source_uri.to_string(),
            ))
            .unwrap();
        writer
            .write_text_chunk(&png::text_metadata::TEXtChunk::new(
                "Thumb::MTime".to_string(),
                source_modified_secs.to_string(),
            ))
            .unwrap();
    }

    fn png_text_chunks_for_test(path: &Path) -> Vec<(String, String)> {
        let mut chunks = super::png_text_chunks(path).unwrap();
        super::png_tail_text_chunks(path, &mut chunks).unwrap();
        chunks
    }

    #[test]
    fn recognizes_supported_image_extensions() {
        assert!(is_builtin_thumbnail_candidate(Path::new("photo.JPG")));
        assert!(is_builtin_thumbnail_candidate(Path::new("photo.webp")));
        assert!(!is_builtin_thumbnail_candidate(Path::new("notes.txt")));
    }

    #[test]
    fn unknown_extensions_do_not_enter_thumbnailer_lookup() {
        assert_eq!(
            thumbnailer_mime_from_extension(Path::new("sysctl.conf").extension()),
            None
        );
        assert!(!is_thumbnail_candidate(Path::new("sysctl.conf")));
        assert!(!is_thumbnail_candidate(Path::new("passwd")));
    }

    #[test]
    fn parses_thumbnailer_entry_and_matches_mime_types() {
        let entry = parse_thumbnailer_entry(
            "[Thumbnailer Entry]\n\
             TryExec=pdf-thumb\n\
             Exec=pdf-thumb %u %o %s\n\
             MimeType=application/pdf;image/svg+xml;video/*;\n",
        )
        .unwrap();

        assert_eq!(entry.try_exec.as_deref(), Some("pdf-thumb"));
        assert_eq!(entry.exec, "pdf-thumb %u %o %s");
        assert_eq!(
            entry.mime_types,
            vec!["application/pdf", "image/svg+xml", "video/*"]
        );

        let entries = vec![ThumbnailerEntry {
            exec: entry.exec,
            mime_types: entry.mime_types,
        }];
        assert!(thumbnailer_for_mime(&entries, "application/pdf").is_some());
        assert!(thumbnailer_for_mime(&entries, "video/mp4").is_some());
        assert!(thumbnailer_for_mime(&entries, "text/plain").is_none());
        assert!(thumbnailer_candidate_for_path(Path::new("document.pdf"), &entries).is_some());
    }

    #[test]
    fn thumbnailer_exec_expands_freedesktop_field_codes_without_shell() {
        assert_eq!(
            expand_thumbnailer_exec(
                "thumb --input %i --uri %u --output %o --size %s --literal %%",
                Path::new("/tmp/source file.pdf"),
                "file:///tmp/source%20file.pdf",
                Path::new("/cache/thumb.png"),
                256,
            )
            .unwrap(),
            vec![
                "thumb",
                "--input",
                "/tmp/source file.pdf",
                "--uri",
                "file:///tmp/source%20file.pdf",
                "--output",
                "/cache/thumb.png",
                "--size",
                "256",
                "--literal",
                "%",
            ]
        );
        assert!(
            expand_thumbnailer_exec(
                "thumb %x",
                Path::new("/tmp/a"),
                "file:///tmp/a",
                Path::new("/tmp/out"),
                128
            )
            .is_err()
        );
    }

    #[test]
    fn thumbnailer_search_dirs_follow_xdg_data_locations() {
        assert_eq!(
            thumbnailer_search_dirs_from_values(
                Some(Path::new("/xdg-data")),
                Some(Path::new("/home/user")),
                Some(Path::new("/opt/share:/usr/share")),
            ),
            vec![
                PathBuf::from("/xdg-data/thumbnailers"),
                PathBuf::from("/opt/share/thumbnailers"),
                PathBuf::from("/usr/share/thumbnailers"),
            ]
        );
        assert_eq!(
            thumbnailer_search_dirs_from_values(None, Some(Path::new("/home/user")), None)[0],
            PathBuf::from("/home/user/.local/share/thumbnailers")
        );
    }

    #[test]
    fn discovers_thumbnailer_entries_and_honors_try_exec() {
        let dir = test_dir("thumbnailer-discovery");
        let thumbnailers = dir.join("thumbnailers");
        fs::create_dir_all(&thumbnailers).unwrap();
        let helper = dir.join("helper");
        fs::write(&helper, "").unwrap();
        fs::write(
            thumbnailers.join("pdf.thumbnailer"),
            format!(
                "[Thumbnailer Entry]\nTryExec={}\nExec={} %u %o %s\nMimeType=application/pdf;\n",
                helper.display(),
                helper.display()
            ),
        )
        .unwrap();
        fs::write(
            thumbnailers.join("missing.thumbnailer"),
            "[Thumbnailer Entry]\nTryExec=definitely-missing-fika-thumb\nExec=missing %u %o %s\nMimeType=image/svg+xml;\n",
        )
        .unwrap();
        fs::write(
            thumbnailers.join("invalid.thumbnailer"),
            "[Thumbnailer Entry]\nExec=broken %u %o\n",
        )
        .unwrap();

        let entries = discover_thumbnailers(std::slice::from_ref(&thumbnailers));

        assert_eq!(entries.len(), 1);
        assert!(thumbnailer_for_mime(&entries, "application/pdf").is_some());
        assert!(thumbnailer_for_mime(&entries, "image/svg+xml").is_none());
        let _ = fs::remove_dir_all(dir);
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

    #[test]
    fn load_thumbnail_writes_freedesktop_cache_after_decode() {
        let dir = test_dir("cache-write");
        let source = dir.join("source.png");
        let cache_base = dir.join("cache").join("thumbnails");
        write_rgba_png(&source, &[0, 255, 0, 255], 1, 1);

        let load = load_thumbnail_with_cache_base(source.clone(), 64, Some(&cache_base));
        let data = load.data.unwrap();
        let cache_paths = load.cache_paths.unwrap();

        assert_eq!(data.rgba, vec![0, 255, 0, 255]);
        assert!(cache_paths.thumbnail_path.exists());
        assert!(thumbnail_disk_entry_is_fresh(
            &cache_paths.thumbnail_path,
            key_for(&source, 64).unwrap().modified_secs
        ));
        let text_chunks = png_text_chunks_for_test(&cache_paths.thumbnail_path);
        assert!(text_chunks.contains(&("Thumb::URI".to_string(), cache_paths.source_uri)));
        assert!(text_chunks.iter().any(|(keyword, text)| {
            keyword == "Thumb::MTime"
                && text == &key_for(&source, 64).unwrap().modified_secs.to_string()
        }));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_thumbnail_prefers_fresh_freedesktop_cache() {
        let dir = test_dir("cache-hit");
        let source = dir.join("source.png");
        let cache_base = dir.join("cache").join("thumbnails");
        write_rgba_png(&source, &[0, 0, 255, 255], 1, 1);
        let cache_paths =
            freedesktop_thumbnail_cache_paths_for_path_base(&source, 64, &cache_base).unwrap();
        write_thumbnail_cache(
            &cache_paths,
            &ThumbnailData {
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255],
            },
            key_for(&source, 64).unwrap().modified_secs,
        )
        .unwrap();

        let load = load_thumbnail_with_cache_base(source, 64, Some(&cache_base));
        let data = load.data.unwrap();

        assert_eq!(data.rgba, vec![255, 0, 0, 255]);
        assert_eq!(
            load.cache_paths.unwrap().thumbnail_path,
            cache_paths.thumbnail_path
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_thumbnail_rejects_cache_with_wrong_freedesktop_uri() {
        let dir = test_dir("cache-wrong-uri");
        let source = dir.join("source.png");
        let cache_base = dir.join("cache").join("thumbnails");
        write_rgba_png(&source, &[0, 255, 0, 255], 1, 1);
        let source_mtime = key_for(&source, 64).unwrap().modified_secs;
        let cache_paths =
            freedesktop_thumbnail_cache_paths_for_path_base(&source, 64, &cache_base).unwrap();
        write_test_cache_png(
            &cache_paths,
            &ThumbnailData {
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255],
            },
            "file:///tmp/wrong-source.png",
            source_mtime,
        );

        let load = load_thumbnail_with_cache_base(source.clone(), 64, Some(&cache_base));
        let data = load.data.unwrap();

        assert_eq!(data.rgba, vec![0, 255, 0, 255]);
        assert!(thumbnail_cache_metadata_matches(
            &cache_paths.thumbnail_path,
            &cache_paths.source_uri,
            source_mtime
        ));
        assert!(
            !png_text_chunks_for_test(&cache_paths.thumbnail_path).contains(&(
                "Thumb::URI".to_string(),
                "file:///tmp/wrong-source.png".to_string()
            ))
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_thumbnail_rejects_cache_with_wrong_freedesktop_mtime() {
        let dir = test_dir("cache-wrong-mtime");
        let source = dir.join("source.png");
        let cache_base = dir.join("cache").join("thumbnails");
        write_rgba_png(&source, &[0, 255, 0, 255], 1, 1);
        let source_mtime = key_for(&source, 64).unwrap().modified_secs;
        let cache_paths =
            freedesktop_thumbnail_cache_paths_for_path_base(&source, 64, &cache_base).unwrap();
        write_test_cache_png(
            &cache_paths,
            &ThumbnailData {
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255],
            },
            &cache_paths.source_uri,
            source_mtime.saturating_add(1),
        );

        let load = load_thumbnail_with_cache_base(source.clone(), 64, Some(&cache_base));
        let data = load.data.unwrap();

        assert_eq!(data.rgba, vec![0, 255, 0, 255]);
        assert!(thumbnail_cache_metadata_matches(
            &cache_paths.thumbnail_path,
            &cache_paths.source_uri,
            source_mtime
        ));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_thumbnail_accepts_freedesktop_cache_with_tail_metadata() {
        let dir = test_dir("cache-tail-metadata");
        let source = dir.join("source.png");
        let cache_base = dir.join("cache").join("thumbnails");
        write_rgba_png(&source, &[0, 0, 255, 255], 1, 1);
        let source_mtime = key_for(&source, 64).unwrap().modified_secs;
        let cache_paths =
            freedesktop_thumbnail_cache_paths_for_path_base(&source, 64, &cache_base).unwrap();
        write_test_cache_png_with_tail_metadata(
            &cache_paths,
            &ThumbnailData {
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255],
            },
            &cache_paths.source_uri,
            source_mtime,
        );

        let load = load_thumbnail_with_cache_base(source, 64, Some(&cache_base));
        let data = load.data.unwrap();

        assert_eq!(data.rgba, vec![255, 0, 0, 255]);
        assert!(thumbnail_cache_metadata_matches(
            &cache_paths.thumbnail_path,
            &cache_paths.source_uri,
            source_mtime
        ));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn fresh_fail_marker_skips_thumbnail_decode() {
        let dir = test_dir("fail-marker-hit");
        let source = dir.join("source.png");
        let cache_base = dir.join("cache").join("thumbnails");
        write_rgba_png(&source, &[0, 0, 255, 255], 1, 1);
        let cache_paths =
            freedesktop_thumbnail_cache_paths_for_path_base(&source, 64, &cache_base).unwrap();
        write_thumbnail_fail_marker(&cache_paths).unwrap();

        let load = load_thumbnail_with_cache_base(source, 64, Some(&cache_base));
        let error = load.data.unwrap_err().to_string();

        assert!(error.contains("thumbnail generation previously failed"));
        assert!(cache_paths.fail_marker_path.exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn decode_failure_writes_freedesktop_fail_marker() {
        let dir = test_dir("fail-marker-write");
        let source = dir.join("broken.png");
        let cache_base = dir.join("cache").join("thumbnails");
        fs::write(&source, b"not an image").unwrap();

        let load = load_thumbnail_with_cache_base(source, 64, Some(&cache_base));
        let cache_paths = load.cache_paths.unwrap();

        assert!(load.data.is_err());
        assert!(cache_paths.fail_marker_path.exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn thumbnail_disk_entry_freshness_uses_source_mtime() {
        assert!(thumbnail_disk_entry_is_fresh_for_times(10, 10));
        assert!(thumbnail_disk_entry_is_fresh_for_times(11, 10));
        assert!(!thumbnail_disk_entry_is_fresh_for_times(9, 10));
    }
}
