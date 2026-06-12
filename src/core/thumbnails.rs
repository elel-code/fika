use super::{
    entries::ItemId,
    pane::{Generation, PaneId},
};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

const THUMBNAILS_DIR: &str = "thumbnails";
const NORMAL_DIR: &str = "normal";
const LARGE_DIR: &str = "large";
const FAIL_DIR: &str = "fail";
const FAIL_APP_DIR: &str = "gnome-thumbnail-factory";
const PNG_EXTENSION: &str = "png";
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const PNG_CHUNK_HEADER_LEN: usize = 8;
const PNG_CHUNK_CRC_LEN: usize = 4;

const FAILURE_THUMBNAIL_PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D', b'R',
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0a, b'I', b'D', b'A', b'T', 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xae,
    0x42, 0x60, 0x82,
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThumbnailSize {
    Normal,
    Large,
}

impl ThumbnailSize {
    pub fn cache_dir(self) -> &'static str {
        match self {
            Self::Normal => NORMAL_DIR,
            Self::Large => LARGE_DIR,
        }
    }

    pub fn max_dimension(self) -> u16 {
        match self {
            Self::Normal => 128,
            Self::Large => 256,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailCacheHit {
    size: ThumbnailSize,
    path: PathBuf,
}

impl ThumbnailCacheHit {
    pub fn size(&self) -> ThumbnailSize {
        self.size
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailCachePaths {
    pub normal: PathBuf,
    pub large: PathBuf,
    pub failure: PathBuf,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThumbnailMetadata {
    pub uri: Option<String>,
    pub mtime: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThumbnailRequestPriority {
    Visible,
    Deferred,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailRequest {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    path: PathBuf,
    uri: String,
    modified_secs: u64,
    priority: ThumbnailRequestPriority,
}

impl ThumbnailRequest {
    pub fn new(
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        priority: ThumbnailRequestPriority,
    ) -> Option<Self> {
        let uri = thumbnail_uri_for_path(&path)?;
        let modified_secs = file_modified_secs(&path)?;
        Some(Self {
            pane_id,
            generation,
            item_id,
            path,
            uri,
            modified_secs,
            priority,
        })
    }

    pub fn pane_id(&self) -> PaneId {
        self.pane_id
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn item_id(&self) -> ItemId {
        self.item_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn modified_secs(&self) -> u64 {
        self.modified_secs
    }

    pub fn priority(&self) -> ThumbnailRequestPriority {
        self.priority
    }

    fn key(&self) -> ThumbnailRequestKey {
        ThumbnailRequestKey {
            pane_id: self.pane_id,
            generation: self.generation,
            item_id: self.item_id,
            uri: self.uri.clone(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ThumbnailRequestQueue {
    visible: VecDeque<ThumbnailRequest>,
    deferred: VecDeque<ThumbnailRequest>,
    pending: HashMap<ThumbnailRequestKey, ThumbnailRequestPriority>,
}

impl ThumbnailRequestQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn enqueue(&mut self, request: ThumbnailRequest) -> bool {
        let key = request.key();
        match self.pending.get(&key).copied() {
            Some(ThumbnailRequestPriority::Visible) => false,
            Some(ThumbnailRequestPriority::Deferred) => {
                if request.priority != ThumbnailRequestPriority::Visible {
                    return false;
                }
                self.deferred.retain(|existing| existing.key() != key);
                self.visible.push_back(request);
                self.pending.insert(key, ThumbnailRequestPriority::Visible);
                true
            }
            None => {
                let priority = request.priority;
                match priority {
                    ThumbnailRequestPriority::Visible => self.visible.push_back(request),
                    ThumbnailRequestPriority::Deferred => self.deferred.push_back(request),
                }
                self.pending.insert(key, priority);
                true
            }
        }
    }

    pub fn enqueue_path(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        item_id: ItemId,
        path: PathBuf,
        priority: ThumbnailRequestPriority,
    ) -> bool {
        ThumbnailRequest::new(pane_id, generation, item_id, path, priority)
            .is_some_and(|request| self.enqueue(request))
    }

    pub fn pop_next(&mut self) -> Option<ThumbnailRequest> {
        let request = self
            .visible
            .pop_front()
            .or_else(|| self.deferred.pop_front())?;
        self.pending.remove(&request.key());
        Some(request)
    }

    pub fn cancel_stale_generations(
        &mut self,
        pane_id: PaneId,
        current_generation: Generation,
    ) -> usize {
        self.remove_matching(|request| {
            request.pane_id == pane_id && request.generation != current_generation
        })
    }

    pub fn cancel_pane(&mut self, pane_id: PaneId) -> usize {
        self.remove_matching(|request| request.pane_id == pane_id)
    }

    fn remove_matching(&mut self, predicate: impl Fn(&ThumbnailRequest) -> bool) -> usize {
        let mut removed = remove_matching_from_queue(&mut self.visible, &predicate);
        removed.extend(remove_matching_from_queue(&mut self.deferred, &predicate));
        for key in &removed {
            self.pending.remove(key);
        }
        removed.len()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ThumbnailRequestKey {
    pane_id: PaneId,
    generation: Generation,
    item_id: ItemId,
    uri: String,
}

pub fn default_thumbnail_cache_root() -> PathBuf {
    default_cache_home().join(THUMBNAILS_DIR)
}

pub fn thumbnail_cache_root(cache_home: &Path) -> PathBuf {
    cache_home.join(THUMBNAILS_DIR)
}

pub fn thumbnail_uri_for_path(path: &Path) -> Option<String> {
    path.is_absolute().then(|| {
        let mut uri = String::from("file://");
        uri.push_str(&percent_encode_path(path));
        uri
    })
}

pub fn thumbnail_cache_key(uri: &str) -> String {
    md5_hex(uri.as_bytes())
}

pub fn thumbnail_cache_path(root: &Path, size: ThumbnailSize, uri: &str) -> PathBuf {
    root.join(size.cache_dir())
        .join(format!("{}.{}", thumbnail_cache_key(uri), PNG_EXTENSION))
}

pub fn thumbnail_failure_path(root: &Path, uri: &str) -> PathBuf {
    root.join(FAIL_DIR).join(FAIL_APP_DIR).join(format!(
        "{}.{}",
        thumbnail_cache_key(uri),
        PNG_EXTENSION
    ))
}

pub fn thumbnail_cache_paths_for_uri(root: &Path, uri: &str) -> ThumbnailCachePaths {
    ThumbnailCachePaths {
        normal: thumbnail_cache_path(root, ThumbnailSize::Normal, uri),
        large: thumbnail_cache_path(root, ThumbnailSize::Large, uri),
        failure: thumbnail_failure_path(root, uri),
    }
}

pub fn cached_thumbnail_for_uri(root: &Path, uri: &str) -> Option<ThumbnailCacheHit> {
    cached_thumbnail(root, uri, None)
}

pub fn cached_thumbnail_for_path(root: &Path, path: &Path) -> Option<ThumbnailCacheHit> {
    let uri = thumbnail_uri_for_path(path)?;
    let modified_secs = file_modified_secs(path)?;
    cached_thumbnail(root, &uri, Some(modified_secs))
}

pub fn thumbnail_metadata(path: &Path) -> io::Result<ThumbnailMetadata> {
    thumbnail_metadata_from_bytes(&fs::read(path)?)
}

fn cached_thumbnail(
    root: &Path,
    uri: &str,
    modified_secs: Option<u64>,
) -> Option<ThumbnailCacheHit> {
    [ThumbnailSize::Normal, ThumbnailSize::Large]
        .into_iter()
        .find_map(|size| {
            let path = thumbnail_cache_path(root, size, uri);
            thumbnail_metadata_matches(&path, uri, modified_secs)
                .then_some(ThumbnailCacheHit { size, path })
        })
}

fn thumbnail_metadata_matches(path: &Path, uri: &str, modified_secs: Option<u64>) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(metadata) = thumbnail_metadata(path) else {
        return false;
    };
    if metadata.uri.as_deref() != Some(uri) {
        return false;
    }
    modified_secs.is_none_or(|expected| metadata.mtime == Some(expected))
}

fn file_modified_secs(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_secs())
        })
}

fn remove_matching_from_queue(
    queue: &mut VecDeque<ThumbnailRequest>,
    predicate: &impl Fn(&ThumbnailRequest) -> bool,
) -> Vec<ThumbnailRequestKey> {
    let mut removed = Vec::new();
    queue.retain(|request| {
        if predicate(request) {
            removed.push(request.key());
            false
        } else {
            true
        }
    });
    removed
}

pub fn thumbnail_failure_is_cached(root: &Path, uri: &str) -> bool {
    thumbnail_failure_path(root, uri).is_file()
}

pub fn record_thumbnail_failure(root: &Path, uri: &str) -> io::Result<PathBuf> {
    let path = thumbnail_failure_path(root, uri);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, FAILURE_THUMBNAIL_PNG)?;
    }
    Ok(path)
}

fn default_cache_home() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".cache"))
}

fn percent_encode_path(path: &Path) -> String {
    let bytes = path_bytes(path);
    let mut encoded = String::new();
    for byte in bytes {
        if uri_path_byte_is_unreserved(byte) || byte == b'/' {
            encoded.push(byte as char);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

fn uri_path_byte_is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn thumbnail_metadata_from_bytes(bytes: &[u8]) -> io::Result<ThumbnailMetadata> {
    if bytes.len() < PNG_SIGNATURE.len() || &bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thumbnail is not a PNG file",
        ));
    }

    let mut metadata = ThumbnailMetadata::default();
    let mut offset = PNG_SIGNATURE.len();
    while bytes.len().saturating_sub(offset) >= PNG_CHUNK_HEADER_LEN {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let chunk_type = &bytes[offset + 4..offset + 8];
        offset += PNG_CHUNK_HEADER_LEN;
        let Some(data_end) = offset.checked_add(length) else {
            return Err(invalid_png_thumbnail());
        };
        let Some(next_offset) = data_end.checked_add(PNG_CHUNK_CRC_LEN) else {
            return Err(invalid_png_thumbnail());
        };
        if next_offset > bytes.len() {
            return Err(invalid_png_thumbnail());
        }

        let data = &bytes[offset..data_end];
        if chunk_type == b"tEXt" {
            read_thumbnail_text_chunk(data, &mut metadata);
        }

        offset = next_offset;
        if chunk_type == b"IEND" {
            break;
        }
    }
    Ok(metadata)
}

fn read_thumbnail_text_chunk(data: &[u8], metadata: &mut ThumbnailMetadata) {
    let Some(separator) = data.iter().position(|byte| *byte == 0) else {
        return;
    };
    let key = &data[..separator];
    let value = &data[separator + 1..];
    match key {
        b"Thumb::URI" => {
            if let Ok(uri) = std::str::from_utf8(value) {
                metadata.uri = Some(uri.to_string());
            }
        }
        b"Thumb::MTime" => {
            if let Ok(value) = std::str::from_utf8(value)
                && let Ok(mtime) = value.parse::<u64>()
            {
                metadata.mtime = Some(mtime);
            }
        }
        _ => {}
    }
}

fn invalid_png_thumbnail() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "thumbnail PNG has truncated chunk data",
    )
}

fn md5_hex(input: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    let mut message = input.to_vec();
    let bit_len = (message.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend(bit_len.to_le_bytes());

    let mut a0 = 0x67452301u32;
    let mut b0 = 0xefcdab89u32;
    let mut c0 = 0x98badcfeu32;
    let mut d0 = 0x10325476u32;

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
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let next = d;
            d = c;
            c = b;
            b = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(K[i])
                    .wrapping_add(words[g])
                    .rotate_left(S[i]),
            );
            a = next;
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
    bytes_to_hex(&digest)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;

    #[test]
    fn thumbnail_uri_percent_encodes_file_path() {
        let uri = thumbnail_uri_for_path(Path::new("/tmp/Fika Test/value#1.txt")).unwrap();
        assert_eq!(uri, "file:///tmp/Fika%20Test/value%231.txt");
        assert!(thumbnail_uri_for_path(Path::new("relative.txt")).is_none());
    }

    #[test]
    fn freedesktop_thumbnail_hash_uses_md5_uri() {
        assert_eq!(thumbnail_cache_key(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            thumbnail_cache_key("abc"),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            thumbnail_cache_key("file:///tmp/Fika%20Test/value%231.txt"),
            "4869a68b8abd00bef4bb8d34392b25c7"
        );
    }

    #[test]
    fn thumbnail_cache_lookup_prefers_normal_before_large() {
        let root = temp_root("lookup");
        let uri = "file:///tmp/image.png";
        let large = thumbnail_cache_path(&root, ThumbnailSize::Large, uri);
        let normal = thumbnail_cache_path(&root, ThumbnailSize::Normal, uri);
        fs::create_dir_all(large.parent().unwrap()).unwrap();
        fs::write(&large, test_thumbnail_png(uri, 123)).unwrap();

        let large_hit = cached_thumbnail_for_uri(&root, uri).unwrap();
        assert_eq!(large_hit.size(), ThumbnailSize::Large);
        assert_eq!(large_hit.path(), large.as_path());

        fs::create_dir_all(normal.parent().unwrap()).unwrap();
        fs::write(&normal, test_thumbnail_png(uri, 123)).unwrap();
        let normal_hit = cached_thumbnail_for_uri(&root, uri).unwrap();
        assert_eq!(normal_hit.size(), ThumbnailSize::Normal);
        assert_eq!(normal_hit.path(), normal.as_path());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_cache_lookup_rejects_mismatched_metadata() {
        let root = temp_root("mismatch");
        let file = root.join("image.png");
        fs::create_dir_all(&root).unwrap();
        fs::write(&file, b"source").unwrap();
        let uri = thumbnail_uri_for_path(&file).unwrap();
        let mtime = fs::metadata(&file)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let thumbnail = thumbnail_cache_path(&root, ThumbnailSize::Normal, &uri);
        fs::create_dir_all(thumbnail.parent().unwrap()).unwrap();
        fs::write(
            &thumbnail,
            test_thumbnail_png("file:///tmp/other.png", mtime),
        )
        .unwrap();
        assert!(cached_thumbnail_for_path(&root, &file).is_none());

        fs::write(&thumbnail, test_thumbnail_png(&uri, mtime + 1)).unwrap();
        assert!(cached_thumbnail_for_path(&root, &file).is_none());

        fs::write(&thumbnail, test_thumbnail_png(&uri, mtime)).unwrap();
        assert_eq!(
            cached_thumbnail_for_path(&root, &file).unwrap().path(),
            thumbnail.as_path()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_metadata_reads_freedesktop_text_chunks() {
        let uri = "file:///tmp/image.png";
        let metadata = thumbnail_metadata_from_bytes(&test_thumbnail_png(uri, 42)).unwrap();

        assert_eq!(metadata.uri.as_deref(), Some(uri));
        assert_eq!(metadata.mtime, Some(42));
    }

    #[test]
    fn thumbnail_request_queue_schedules_visible_before_deferred() {
        let root = temp_root("queue-visible");
        fs::create_dir_all(&root).unwrap();
        let mut queue = ThumbnailRequestQueue::new();

        assert!(queue.enqueue(test_request(
            &root,
            "deferred.png",
            ItemId(1),
            Generation(1),
            ThumbnailRequestPriority::Deferred,
        )));
        assert!(queue.enqueue(test_request(
            &root,
            "visible.png",
            ItemId(2),
            Generation(1),
            ThumbnailRequestPriority::Visible,
        )));

        let first = queue.pop_next().unwrap();
        assert_eq!(first.item_id(), ItemId(2));
        assert_eq!(first.priority(), ThumbnailRequestPriority::Visible);
        assert_eq!(queue.pop_next().unwrap().item_id(), ItemId(1));
        assert!(queue.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_request_queue_deduplicates_and_promotes_visible_requests() {
        let root = temp_root("queue-dedup");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("same.png");
        fs::write(&path, b"source").unwrap();
        let mut queue = ThumbnailRequestQueue::new();

        assert!(queue.enqueue_path(
            PaneId(1),
            Generation(1),
            ItemId(1),
            path.clone(),
            ThumbnailRequestPriority::Deferred,
        ));
        assert!(!queue.enqueue_path(
            PaneId(1),
            Generation(1),
            ItemId(1),
            path.clone(),
            ThumbnailRequestPriority::Deferred,
        ));
        assert!(queue.enqueue_path(
            PaneId(1),
            Generation(1),
            ItemId(1),
            path,
            ThumbnailRequestPriority::Visible,
        ));
        assert_eq!(queue.len(), 1);

        let request = queue.pop_next().unwrap();
        assert_eq!(request.item_id(), ItemId(1));
        assert_eq!(request.priority(), ThumbnailRequestPriority::Visible);
        assert!(queue.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_request_queue_cancels_stale_generations_for_navigation() {
        let root = temp_root("queue-cancel");
        fs::create_dir_all(&root).unwrap();
        let mut queue = ThumbnailRequestQueue::new();

        assert!(queue.enqueue(test_request(
            &root,
            "old.png",
            ItemId(1),
            Generation(1),
            ThumbnailRequestPriority::Visible,
        )));
        assert!(queue.enqueue(test_request(
            &root,
            "current.png",
            ItemId(2),
            Generation(2),
            ThumbnailRequestPriority::Deferred,
        )));
        assert!(
            queue.enqueue(
                ThumbnailRequest::new(
                    PaneId(2),
                    Generation(1),
                    ItemId(3),
                    write_source(&root, "other-pane.png"),
                    ThumbnailRequestPriority::Visible,
                )
                .unwrap(),
            )
        );

        assert_eq!(queue.cancel_stale_generations(PaneId(1), Generation(2)), 1);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.pop_next().unwrap().pane_id(), PaneId(2));
        let current = queue.pop_next().unwrap();
        assert_eq!(current.pane_id(), PaneId(1));
        assert_eq!(current.generation(), Generation(2));
        assert!(queue.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thumbnail_failure_marker_uses_freedesktop_fail_path() {
        let root = temp_root("failure");
        let uri = "file:///tmp/broken.png";
        let path = record_thumbnail_failure(&root, uri).unwrap();

        assert_eq!(path, thumbnail_failure_path(&root, uri));
        assert!(thumbnail_failure_is_cached(&root, uri));
        let bytes = fs::read(path).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));

        let _ = fs::remove_dir_all(root);
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!("fika-thumbnail-{name}-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn test_request(
        root: &Path,
        name: &str,
        item_id: ItemId,
        generation: Generation,
        priority: ThumbnailRequestPriority,
    ) -> ThumbnailRequest {
        ThumbnailRequest::new(
            PaneId(1),
            generation,
            item_id,
            write_source(root, name),
            priority,
        )
        .unwrap()
    }

    fn write_source(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        fs::write(&path, b"source").unwrap();
        path
    }

    pub(crate) fn test_thumbnail_png(uri: &str, mtime: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(PNG_SIGNATURE);
        bytes.extend(test_png_chunk(b"IHDR", &[0; 13]));
        bytes.extend(test_png_text_chunk("Thumb::URI", uri));
        bytes.extend(test_png_text_chunk("Thumb::MTime", &mtime.to_string()));
        bytes.extend(test_png_chunk(b"IEND", &[]));
        bytes
    }

    fn test_png_text_chunk(key: &str, value: &str) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(key.as_bytes());
        data.push(0);
        data.extend(value.as_bytes());
        test_png_chunk(b"tEXt", &data)
    }

    fn test_png_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = Vec::new();
        chunk.extend((data.len() as u32).to_be_bytes());
        chunk.extend(chunk_type);
        chunk.extend(data);
        chunk.extend([0; 4]);
        chunk
    }
}
