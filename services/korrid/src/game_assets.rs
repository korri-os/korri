use image::{GenericImageView, ImageFormat, ImageReader, Limits};
use korri_surface_treaty::SurfaceMediaKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const SUPPORTED_ASSET_EXTENSIONS: &[&str] = &["png", "jpg", "webp"];

const STATE_DIR: &str = "game-assets";
const BLOBS_DIR: &str = "blobs";
const ASSIGNMENTS_FILE: &str = "assignments.json";
pub(crate) const MAX_ASSET_BYTES: usize = 5 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;
/// The decoder may hold one image of the largest admitted size as RGBA.
const MAX_DECODED_BYTES: u64 = MAX_DIMENSION as u64 * MAX_DIMENSION as u64 * 4;
/// The images korrid keeps of one game, from one plugin, of one kind
/// (ADR 0091; the count is the coordinator's v1 choice).
pub(crate) const MAX_IMAGES_PER_KIND: usize = 4;
/// The shortest side a `picture` may have.
const MIN_PICTURE_SIDE: u32 = 256;

/// One image of one game, in `assignments.json` under
/// `<playable id>:<plugin id>:<kind>:<n>` (legacy
/// `GameAssetAssignmentRecord.id`, widened to hold a list per plugin and
/// kind). `n` is the image's place in the plugin's order, from 0.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AssetAssignment {
    pub asset_id: String,
    pub kind: SurfaceMediaKind,
    pub owner: AssetOwnerIdentity,
    pub source: GameAssetSource,
    /// One colour of this image as lowercase `#rrggbb`, computed once when
    /// the image is stored (`dominant_color`). Absent when the image has no
    /// clear colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A stored image of one game, as the game list carries it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredMedia {
    pub kind: SurfaceMediaKind,
    pub asset_id: String,
    pub width: u32,
    pub height: u32,
    pub color: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AssetOwnerIdentity {
    pub playable_id: String,
    pub release_id: String,
    pub release_fingerprint: String,
    pub rom_identity: String,
}

/// Where a stored image came from: legacy `GameAssetSource { provider, id?,
/// url? }` with main's `sha256`, `width`, `height` and `format`. `provider`
/// is the id of the plugin that found the image. A record with any other
/// field is from before the one-off migration and is not read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GameAssetSource {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

/// Downloaded bytes a plugin proposed for one kind, with what it said of
/// them. The owning plugin's id is given beside them, never by the plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AssetCandidate {
    pub bytes: Vec<u8>,
    pub declared_width: Option<u32>,
    pub declared_height: Option<u32>,
    pub source_id: Option<String>,
    pub source_url: Option<String>,
}

/// What `replace` did with one plugin's images of one kind.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Replaced {
    /// The records now stored, in the plugin's order.
    pub stored: Vec<AssetAssignment>,
    /// Why each refused candidate was refused, in the plugin's order.
    pub rejected: Vec<AssetError>,
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub(crate) enum AssetError {
    #[error("asset bytes exceed the configured limit")]
    Oversized,
    #[error("asset format is unsupported")]
    Unsupported,
    #[error("asset bytes are malformed")]
    Malformed,
    #[error("asset dimensions are unsafe")]
    UnsafeDimensions,
    #[error("asset dimensions do not match provider declarations")]
    InconsistentDimensions,
    #[error("asset storage is unavailable")]
    Storage,
}

/// `app.game-assets.object`: the bytes of one stored image, by its asset id.
/// Like `app.saves.object`, it is a tagged request answered with bytes on an
/// object route (`game_asset_object`), not with a JSON outcome on `/rpc`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameAssetObjectRequest {
    pub asset_id: String,
}

/// One stored image and the media type its asset id names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AssignedAsset {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct GameAssetRepository {
    root: PathBuf,
}

impl GameAssetRepository {
    pub(crate) fn new(private_root: impl AsRef<Path>) -> Self {
        Self {
            root: private_root.as_ref().join(STATE_DIR),
        }
    }

    /// Every record of this exact game, in key order: by plugin id, then
    /// kind, then place.
    #[cfg(test)]
    pub(crate) fn assignments(
        &self,
        owner: &AssetOwnerIdentity,
    ) -> Result<Vec<AssetAssignment>, AssetError> {
        Ok(read_assignments(&self.root)?
            .into_values()
            .filter(|assignment| &assignment.owner == owner)
            .collect())
    }

    /// Whether this exact game holds any image from this plugin, of `kind`
    /// or of any kind.
    pub(crate) fn holds_from(
        &self,
        owner: &AssetOwnerIdentity,
        provider: &str,
        kind: Option<SurfaceMediaKind>,
    ) -> Result<bool, AssetError> {
        Ok(read_assignments(&self.root)?.values().any(|assignment| {
            &assignment.owner == owner
                && assignment.source.provider == provider
                && kind.is_none_or(|kind| assignment.kind == kind)
        }))
    }

    #[cfg(test)]
    pub(crate) fn matching_cover_asset_id(
        &self,
        owner: &AssetOwnerIdentity,
    ) -> Result<Option<String>, AssetError> {
        Ok(self
            .matching_cover_asset_ids(std::iter::once(owner))?
            .remove(&owner.playable_id))
    }

    /// The cover of each of these exact games (`LocalGame.coverAssetId`):
    /// the largest of its square-ish pictures, from 0.85 to 1.2 wide for
    /// each unit of height (Velvet's square tile class, ADR 0091 slice 4).
    /// Of two of the same area, the first in key order. A game with no
    /// square-ish picture has no cover. An image whose blob is gone is left
    /// out.
    pub(crate) fn matching_cover_asset_ids<'a>(
        &self,
        owners: impl IntoIterator<Item = &'a AssetOwnerIdentity>,
    ) -> Result<BTreeMap<String, String>, AssetError> {
        let assignments = read_assignments(&self.root)?;
        let by_game = by_playable_id(&assignments);
        let blob_root = self.root.join(BLOBS_DIR);
        let mut covers = BTreeMap::new();
        for owner in owners {
            let mut best: Option<(u64, &AssetAssignment)> = None;
            for assignment in by_game
                .get(owner.playable_id.as_str())
                .into_iter()
                .flatten()
            {
                if &assignment.owner != owner
                    || assignment.kind != SurfaceMediaKind::Picture
                    || !square_ish(assignment.source.width, assignment.source.height)
                    || !stored_blob(&blob_root, &assignment.asset_id)
                {
                    continue;
                }
                let area = u64::from(assignment.source.width) * u64::from(assignment.source.height);
                if best.is_none_or(|(largest, _)| area > largest) {
                    best = Some((area, assignment));
                }
            }
            if let Some((_, assignment)) = best {
                covers.insert(owner.playable_id.clone(), assignment.asset_id.clone());
            }
        }
        Ok(covers)
    }

    /// Every stored image of each of these exact games, of every kind and
    /// from every plugin, in key order, by playable id. A game with none is
    /// not in the map. An image whose blob is gone is left out.
    pub(crate) fn matching_media<'a>(
        &self,
        owners: impl IntoIterator<Item = &'a AssetOwnerIdentity>,
    ) -> Result<BTreeMap<String, Vec<StoredMedia>>, AssetError> {
        let assignments = read_assignments(&self.root)?;
        let by_game = by_playable_id(&assignments);
        let blob_root = self.root.join(BLOBS_DIR);
        let mut media = BTreeMap::new();
        for owner in owners {
            let stored: Vec<StoredMedia> = by_game
                .get(owner.playable_id.as_str())
                .into_iter()
                .flatten()
                .filter(|assignment| {
                    &assignment.owner == owner && stored_blob(&blob_root, &assignment.asset_id)
                })
                .map(|assignment| StoredMedia {
                    kind: assignment.kind,
                    asset_id: assignment.asset_id.clone(),
                    width: assignment.source.width,
                    height: assignment.source.height,
                    color: assignment.color.clone(),
                })
                .collect();
            if !stored.is_empty() {
                media.insert(owner.playable_id.clone(), stored);
            }
        }
        Ok(media)
    }

    /// The image an assignment names, for `app.game-assets.object`.
    ///
    /// The rules are those of `KorriGameAssetPathHandler` at `android-final`:
    /// the id is one well-formed content address, an assignment record must
    /// name it, and the blob is read from the blob folder by that id alone.
    /// A request path is never joined to the private root.
    pub(crate) fn assigned_asset(
        &self,
        asset_id: &str,
    ) -> Result<Option<AssignedAsset>, AssetError> {
        if !valid_asset_id(asset_id) {
            return Ok(None);
        }
        if !assigned_asset_ids(&self.root)?.contains(asset_id) {
            return Ok(None);
        }
        let blob = self.root.join(BLOBS_DIR).join(asset_id);
        let bytes = match fs::read(&blob) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(AssetError::Storage),
        };
        if bytes.len() > MAX_ASSET_BYTES {
            return Err(AssetError::Oversized);
        }
        Ok(Some(AssignedAsset {
            bytes,
            content_type: content_type(asset_id),
        }))
    }

    /// Check each candidate against the kind's rules, in order, and keep
    /// the first `MAX_IMAGES_PER_KIND` that pass as this game's images of
    /// that kind from `provider`. They replace that plugin's earlier images
    /// of the kind for this playable id, in one write. When none passes,
    /// nothing is written and the earlier images stay.
    pub(crate) fn replace(
        &self,
        owner: AssetOwnerIdentity,
        provider: &str,
        kind: SurfaceMediaKind,
        candidates: Vec<AssetCandidate>,
    ) -> Result<Replaced, AssetError> {
        let mut replaced = Replaced::default();
        let mut admitted = Vec::new();
        for candidate in candidates {
            if admitted.len() == MAX_IMAGES_PER_KIND {
                break;
            }
            match validate_image(&candidate, kind) {
                Ok(decoded) => admitted.push((candidate, decoded)),
                Err(error) => replaced.rejected.push(error),
            }
        }
        if admitted.is_empty() {
            return Ok(replaced);
        }
        let mut assignments = read_assignments(&self.root)?;
        fs::create_dir_all(self.root.join(BLOBS_DIR)).map_err(|_| AssetError::Storage)?;
        let mut written = Vec::new();
        let remove_written = |written: &[PathBuf]| {
            for path in written {
                let _ = fs::remove_file(path);
            }
        };
        for (candidate, decoded) in admitted {
            let blob_name = format!("{}.{}", decoded.sha256, decoded.extension);
            let blob_path = self.root.join(BLOBS_DIR).join(&blob_name);
            if !blob_path.exists() {
                if let Err(error) = write_atomically(&blob_path, &candidate.bytes) {
                    remove_written(&written);
                    return Err(error);
                }
                written.push(blob_path);
            }
            replaced.stored.push(AssetAssignment {
                asset_id: blob_name,
                kind,
                owner: owner.clone(),
                source: GameAssetSource {
                    provider: provider.to_owned(),
                    id: candidate.source_id,
                    url: candidate.source_url,
                    sha256: format!("sha256:{}", decoded.sha256),
                    width: decoded.width,
                    height: decoded.height,
                    format: decoded.extension,
                },
                color: decoded.color,
            });
        }
        assignments.retain(|_, assignment| {
            !(assignment.owner.playable_id == owner.playable_id
                && assignment.source.provider == provider
                && assignment.kind == kind)
        });
        for (place, assignment) in replaced.stored.iter().enumerate() {
            assignments.insert(
                assignment_key(&owner.playable_id, provider, kind, place),
                assignment.clone(),
            );
        }
        if let Err(error) = write_json_atomically(&self.root.join(ASSIGNMENTS_FILE), &assignments) {
            remove_written(&written);
            return Err(error);
        }
        Ok(replaced)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DecodedImage {
    sha256: String,
    extension: String,
    width: u32,
    height: u32,
    color: Option<String>,
}

/// `<playable id>:<plugin id>:<kind>:<n>`, the key of one assignment.
fn assignment_key(
    playable_id: &str,
    provider: &str,
    kind: SurfaceMediaKind,
    place: usize,
) -> String {
    format!("{playable_id}:{provider}:{}:{place}", kind.as_str())
}

/// The records of each playable id, in key order. The key is not parsed:
/// playable ids and plugin ids may both hold `:`.
fn by_playable_id(
    assignments: &BTreeMap<String, AssetAssignment>,
) -> BTreeMap<&str, Vec<&AssetAssignment>> {
    let mut by_game: BTreeMap<&str, Vec<&AssetAssignment>> = BTreeMap::new();
    for assignment in assignments.values() {
        by_game
            .entry(assignment.owner.playable_id.as_str())
            .or_default()
            .push(assignment);
    }
    by_game
}

fn stored_blob(blob_root: &Path, asset_id: &str) -> bool {
    valid_asset_id(asset_id) && blob_root.join(asset_id).is_file()
}

/// From 0.85 to 1.2 wide for each unit of height, in whole numbers.
fn square_ish(width: u32, height: u32) -> bool {
    let (width, height) = (u64::from(width), u64::from(height));
    100 * width >= 85 * height && 10 * width <= 12 * height
}

fn valid_asset_id(asset_id: &str) -> bool {
    let Some((hash, extension)) = asset_id.split_once('.') else {
        return false;
    };
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        && SUPPORTED_ASSET_EXTENSIONS.contains(&extension)
}

/// What identifies one written `assignments.json`: every write replaces the
/// file (`write_atomically`), so a new one has a new modification time.
#[derive(Clone, Debug, Eq, PartialEq)]
struct AssignmentsFileKey {
    path: PathBuf,
    modified: SystemTime,
    length: u64,
}

/// The asset ids the assignment records name. A surface asks for every cover
/// of its window at once, so the records are read once per written file, not
/// once per image (as `KorriGameAssetPathHandler` at `android-final` did).
fn assigned_asset_ids(root: &Path) -> Result<Arc<BTreeSet<String>>, AssetError> {
    static LAST: Mutex<Option<(AssignmentsFileKey, Arc<BTreeSet<String>>)>> = Mutex::new(None);
    let path = root.join(ASSIGNMENTS_FILE);
    let key = match fs::metadata(&path) {
        Ok(metadata) => AssignmentsFileKey {
            modified: metadata.modified().map_err(|_| AssetError::Storage)?,
            length: metadata.len(),
            path,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Arc::default());
        }
        Err(_) => return Err(AssetError::Storage),
    };
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((known, ids)) = last.as_ref() {
        if *known == key {
            return Ok(Arc::clone(ids));
        }
    }
    let ids: Arc<BTreeSet<String>> = Arc::new(
        read_assignments(root)?
            .into_values()
            .map(|assignment| assignment.asset_id)
            .collect(),
    );
    *last = Some((key, Arc::clone(&ids)));
    Ok(ids)
}

/// The media type of a well-formed asset id (`SUPPORTED_ASSET_EXTENSIONS`).
fn content_type(asset_id: &str) -> &'static str {
    match asset_id.rsplit_once('.').map(|(_, extension)| extension) {
        Some("png") => "image/png",
        Some("jpg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

/// How the model names a stored image: this prefix and the asset id. It is
/// the form `coverArtUrl` had for the WebView at `android-final`
/// (`KorriGameAssetPathHandler.ROUTE_PREFIX`). It is a reference, not a
/// route: a surface host reads the image with `app.game-assets.object`.
const ASSET_REFERENCE_PREFIX: &str = "/game-assets/";

/// `SurfaceGame.coverArtUrl` for a stored asset.
pub(crate) fn asset_url(asset_id: &str) -> String {
    format!("{ASSET_REFERENCE_PREFIX}{asset_id}")
}

fn validate_image(
    candidate: &AssetCandidate,
    kind: SurfaceMediaKind,
) -> Result<DecodedImage, AssetError> {
    if candidate.bytes.len() > MAX_ASSET_BYTES {
        return Err(AssetError::Oversized);
    }
    if looks_like_svg(&candidate.bytes)
        || candidate.bytes.starts_with(b"GIF87a")
        || candidate.bytes.starts_with(b"GIF89a")
    {
        return Err(AssetError::Unsupported);
    }
    if is_animated_webp(&candidate.bytes) {
        return Err(AssetError::Unsupported);
    }
    let format = image::guess_format(&candidate.bytes).map_err(|_| AssetError::Unsupported)?;
    let extension = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::WebP => "webp",
        _ => return Err(AssetError::Unsupported),
    };
    let mut header_reader = ImageReader::with_format(Cursor::new(&candidate.bytes), format);
    header_reader.limits(image_limits());
    let (width, height) = header_reader
        .into_dimensions()
        .map_err(image_dimension_error)?;
    if !kind_admits_dimensions(kind, width, height) {
        return Err(AssetError::UnsafeDimensions);
    }
    // A logo lies over other art, so it needs transparency, which only PNG
    // and WebP carry.
    if kind == SurfaceMediaKind::Logo && format == ImageFormat::Jpeg {
        return Err(AssetError::Unsupported);
    }
    let mut reader = ImageReader::with_format(Cursor::new(&candidate.bytes), format);
    reader.limits(image_limits());
    let image = reader.decode().map_err(image_decode_error)?;
    let (width, height) = image.dimensions();
    if !kind_admits_dimensions(kind, width, height) {
        return Err(AssetError::UnsafeDimensions);
    }
    if kind == SurfaceMediaKind::Logo && !has_transparency(&image) {
        return Err(AssetError::Unsupported);
    }
    if candidate
        .declared_width
        .is_some_and(|declared| declared != width)
        || candidate
            .declared_height
            .is_some_and(|declared| declared != height)
    {
        return Err(AssetError::InconsistentDimensions);
    }
    let sha256 = hex::encode(Sha256::digest(&candidate.bytes));
    Ok(DecodedImage {
        sha256,
        extension: extension.into(),
        width,
        height,
        color: dominant_color(&image),
    })
}

/// The side of the square an image is reduced to before its colour is read.
const COLOR_SAMPLE_SIDE: u32 = 32;
/// Hue buckets of 30 degrees.
const HUE_BUCKETS: usize = 12;

/// One colour of an image, as lowercase `#rrggbb`, for a surface to tint
/// with (`SurfaceMedia.color`). The image is reduced to 32 by 32 pixels.
/// Pixels that are mostly transparent, near black, near white or of low
/// saturation are dropped. The rest are put in 12 hue buckets, and the
/// answer is the average colour of the largest bucket. `None` when no pixel
/// is left: a surface then uses its own fallback.
fn dominant_color(image: &image::DynamicImage) -> Option<String> {
    let sample = image
        .resize_exact(
            COLOR_SAMPLE_SIDE,
            COLOR_SAMPLE_SIDE,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    // Per bucket: the pixel count and the sum of each channel.
    let mut buckets = [(0u32, [0u32; 3]); HUE_BUCKETS];
    for pixel in sample.pixels() {
        let [red, green, blue, alpha] = pixel.0;
        let (max, min) = (red.max(green).max(blue), red.min(green).min(blue));
        let near_black = max < 40;
        let near_white = min > 230;
        // Saturation under a quarter: greys, and washed-out colour.
        let low_saturation = u32::from(max - min) * 4 < u32::from(max);
        if alpha < 128 || near_black || near_white || low_saturation {
            continue;
        }
        let (red_f, green_f, blue_f) = (f32::from(red), f32::from(green), f32::from(blue));
        let spread = f32::from(max - min);
        let hue = if max == red {
            60.0 * ((green_f - blue_f) / spread).rem_euclid(6.0)
        } else if max == green {
            60.0 * ((blue_f - red_f) / spread + 2.0)
        } else {
            60.0 * ((red_f - green_f) / spread + 4.0)
        };
        let bucket = (hue / 30.0) as usize % HUE_BUCKETS;
        buckets[bucket].0 += 1;
        for (sum, channel) in buckets[bucket].1.iter_mut().zip([red, green, blue]) {
            *sum += u32::from(channel);
        }
    }
    // The first of equally large buckets, so the answer is always the same.
    let (count, sums) = buckets
        .iter()
        .rev()
        .max_by_key(|(count, _)| *count)
        .copied()?;
    if count == 0 {
        return None;
    }
    let [red, green, blue] = sums.map(|sum| (sum + count / 2) / count);
    Some(format!("#{red:02x}{green:02x}{blue:02x}"))
}

fn image_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    limits
}

/// The sanity rule of each kind (ADR 0091). Every image is at least one
/// and at most 4096 pixels a side.
/// - `picture`: each side at least 256 pixels. Any shape: a surface picks
///   by the measured shape.
/// - `logo`: any shape. It must also have transparency (`has_transparency`).
/// - `icon`: square.
fn kind_admits_dimensions(kind: SurfaceMediaKind, width: u32, height: u32) -> bool {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return false;
    }
    match kind {
        SurfaceMediaKind::Picture => width >= MIN_PICTURE_SIDE && height >= MIN_PICTURE_SIDE,
        SurfaceMediaKind::Logo => true,
        SurfaceMediaKind::Icon => width == height,
    }
}

/// An alpha channel, and at least one pixel that is not fully opaque. An
/// alpha channel alone is not enough: a logo on an opaque box hides the
/// art under it.
fn has_transparency(image: &image::DynamicImage) -> bool {
    image.color().has_alpha() && image.to_rgba8().pixels().any(|pixel| pixel.0[3] < u8::MAX)
}

fn image_dimension_error(error: image::ImageError) -> AssetError {
    match error {
        image::ImageError::Limits(_) => AssetError::UnsafeDimensions,
        image::ImageError::Unsupported(_) => AssetError::Unsupported,
        _ => AssetError::Malformed,
    }
}

fn image_decode_error(error: image::ImageError) -> AssetError {
    match error {
        image::ImageError::Limits(_) => AssetError::UnsafeDimensions,
        image::ImageError::Unsupported(_) => AssetError::Unsupported,
        _ => AssetError::Malformed,
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let prefix_len = bytes.len().min(256);
    let prefix = String::from_utf8_lossy(&bytes[..prefix_len]).to_ascii_lowercase();
    prefix.contains("<svg")
}

fn is_animated_webp(bytes: &[u8]) -> bool {
    bytes.starts_with(b"RIFF")
        && bytes.get(8..12) == Some(b"WEBP")
        && bytes.windows(4).any(|window| window == b"ANIM")
}

fn read_assignments(root: &Path) -> Result<BTreeMap<String, AssetAssignment>, AssetError> {
    let path = root.join(ASSIGNMENTS_FILE);
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| AssetError::Storage),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(_) => Err(AssetError::Storage),
    }
}

fn write_json_atomically<T: Serialize>(path: &Path, value: &T) -> Result<(), AssetError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| AssetError::Storage)?;
    write_atomically(path, &bytes)
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), AssetError> {
    let parent = path.parent().ok_or(AssetError::Storage)?;
    fs::create_dir_all(parent).map_err(|_| AssetError::Storage)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("asset"),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AssetError::Storage)?
            .as_nanos()
    ));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| AssetError::Storage)?;
        file.write_all(bytes).map_err(|_| AssetError::Storage)?;
        file.sync_all().map_err(|_| AssetError::Storage)?;
    }
    fs::rename(&temporary, path).map_err(|_| AssetError::Storage)?;
    sync_directory(parent);
    Ok(())
}

fn sync_directory(path: &Path) {
    if let Ok(file) = fs::File::open(path) {
        let _ = file.sync_all();
    }
}

/// An encoded image of one flat colour for tests here and in the artwork
/// pass: real bytes of a real format. With `alpha`, it has an alpha channel
/// and its top left pixel is fully transparent, as a logo's background is.
#[cfg(test)]
pub(crate) fn test_image(
    width: u32,
    height: u32,
    format: ImageFormat,
    alpha: bool,
    rgb: [u8; 3],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let [r, g, b] = rgb;
    let image = if alpha {
        let mut image = image::RgbaImage::from_pixel(width, height, image::Rgba([r, g, b, 255]));
        image.put_pixel(0, 0, image::Rgba([r, g, b, 0]));
        image::DynamicImage::ImageRgba8(image)
    } else {
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            width,
            height,
            image::Rgb([r, g, b]),
        ))
    };
    image
        .write_to(&mut Cursor::new(&mut bytes), format)
        .expect("test image encodes");
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use SurfaceMediaKind::{Icon, Logo, Picture};

    const PNG_1X1: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 250, 207, 0, 0, 2, 7,
        1, 2, 154, 28, 49, 113, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    const PLUGIN: &str = "@korri:steamgriddb";
    const OTHER: &str = "@korri:igdb";

    fn owner() -> AssetOwnerIdentity {
        AssetOwnerIdentity {
            playable_id: "wl4".into(),
            release_id: "gba".into(),
            release_fingerprint: "sha256:item".into(),
            rom_identity: "sha256:rom".into(),
        }
    }

    fn candidate(bytes: Vec<u8>) -> AssetCandidate {
        AssetCandidate {
            bytes,
            declared_width: None,
            declared_height: None,
            source_id: Some("20".into()),
            source_url: Some("https://cdn2.steamgriddb.com/grid/20.png".into()),
        }
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        test_image(width, height, ImageFormat::Png, true, [200, 40, 40])
    }

    fn file(private: &Path) -> serde_json::Value {
        serde_json::from_slice(&fs::read(private.join(STATE_DIR).join(ASSIGNMENTS_FILE)).unwrap())
            .unwrap()
    }

    fn keys(private: &Path) -> Vec<String> {
        file(private).as_object().unwrap().keys().cloned().collect()
    }

    fn asset_id(bytes: &[u8], extension: &str) -> String {
        format!("{}.{extension}", hex::encode(Sha256::digest(bytes)))
    }

    /// Store `images` as `provider`'s images of `kind` for `owner`, all of
    /// which must pass.
    fn put(
        repo: &GameAssetRepository,
        owner: &AssetOwnerIdentity,
        provider: &str,
        kind: SurfaceMediaKind,
        images: &[Vec<u8>],
    ) -> Vec<AssetAssignment> {
        let replaced = repo
            .replace(
                owner.clone(),
                provider,
                kind,
                images.iter().cloned().map(candidate).collect(),
            )
            .unwrap();
        assert!(replaced.rejected.is_empty(), "{:?}", replaced.rejected);
        replaced.stored
    }

    #[test]
    fn images_are_stored_content_addressably_under_playable_plugin_kind_and_place() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let pictures = [png(512, 512), png(600, 900), png(1920, 620)];
        let logo = png(400, 120);
        put(&repo, &owner(), PLUGIN, Picture, &pictures);
        put(&repo, &owner(), PLUGIN, Logo, std::slice::from_ref(&logo));
        put(&repo, &owner(), OTHER, Picture, &[png(1920, 1080)]);
        assert_eq!(
            keys(private.path()),
            [
                "wl4:@korri:igdb:picture:0",
                "wl4:@korri:steamgriddb:logo:0",
                "wl4:@korri:steamgriddb:picture:0",
                "wl4:@korri:steamgriddb:picture:1",
                "wl4:@korri:steamgriddb:picture:2",
            ]
        );
        // The record: the kind, the owner, and the source as legacy
        // `GameAssetSource` plus what korrid measured.
        let hero = &file(private.path())["wl4:@korri:steamgriddb:picture:2"];
        assert_eq!(hero["kind"], "picture");
        assert!(hero.get("role").is_none());
        assert_eq!(hero["owner"]["playable_id"], "wl4");
        let sha256 = hex::encode(Sha256::digest(&pictures[2]));
        assert_eq!(hero["asset_id"], format!("{sha256}.png"));
        assert_eq!(
            hero["source"],
            serde_json::json!({
                "provider": PLUGIN,
                "id": "20",
                "url": "https://cdn2.steamgriddb.com/grid/20.png",
                "sha256": format!("sha256:{sha256}"),
                "width": 1920,
                "height": 620,
                "format": "png",
            })
        );
        assert_eq!(
            repo.assigned_asset(&asset_id(&logo, "png"))
                .unwrap()
                .unwrap()
                .bytes,
            logo
        );
        assert!(repo.holds_from(&owner(), PLUGIN, None).unwrap());
        assert!(repo.holds_from(&owner(), OTHER, None).unwrap());
        assert!(!repo.holds_from(&owner(), "@korri:other", None).unwrap());
        // A game whose identity changed has no image under the old records.
        let mut changed = owner();
        changed.rom_identity = "sha256:other-rom".into();
        assert!(repo.assignments(&changed).unwrap().is_empty());
        assert!(!repo.holds_from(&changed, PLUGIN, None).unwrap());
        assert!(repo.matching_media([&changed]).unwrap().is_empty());
    }

    #[test]
    fn a_new_answer_replaces_only_that_plugins_images_of_that_kind() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        put(
            &repo,
            &owner(),
            PLUGIN,
            Picture,
            &[png(512, 512), png(600, 900), png(1920, 620)],
        );
        put(&repo, &owner(), PLUGIN, Logo, &[png(400, 120)]);
        put(&repo, &owner(), OTHER, Picture, &[png(1920, 1080)]);
        let other_before = file(private.path())["wl4:@korri:igdb:picture:0"].clone();
        let logo_before = file(private.path())["wl4:@korri:steamgriddb:logo:0"].clone();

        // One picture now: places 1 and 2 go, place 0 is the new image.
        let newer = png(1024, 1024);
        put(
            &repo,
            &owner(),
            PLUGIN,
            Picture,
            std::slice::from_ref(&newer),
        );
        assert_eq!(
            keys(private.path()),
            [
                "wl4:@korri:igdb:picture:0",
                "wl4:@korri:steamgriddb:logo:0",
                "wl4:@korri:steamgriddb:picture:0",
            ]
        );
        assert_eq!(
            file(private.path())["wl4:@korri:steamgriddb:picture:0"]["asset_id"],
            asset_id(&newer, "png")
        );
        assert_eq!(
            file(private.path())["wl4:@korri:igdb:picture:0"],
            other_before
        );
        assert_eq!(
            file(private.path())["wl4:@korri:steamgriddb:logo:0"],
            logo_before
        );

        // A record of the same playable id under an older identity is the
        // plugin's earlier image too, and goes.
        let mut older = owner();
        older.release_fingerprint = "sha256:older".into();
        put(&repo, &older, PLUGIN, Logo, &[png(400, 121)]);
        assert_eq!(repo.assignments(&older).unwrap().len(), 1);
        assert!(repo
            .assignments(&owner())
            .unwrap()
            .iter()
            .all(|assignment| assignment.kind != Logo || assignment.source.provider != PLUGIN));
    }

    #[test]
    fn at_most_four_images_are_kept_in_the_plugins_order_and_refused_ones_do_not_count() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let images: Vec<Vec<u8>> = (0..6).map(|index| png(300 + index, 300)).collect();
        // The second is too small to be a picture: it is refused and the
        // next one takes its place.
        let mut candidates: Vec<AssetCandidate> = images.iter().cloned().map(candidate).collect();
        candidates.insert(1, candidate(png(255, 300)));
        let replaced = repo.replace(owner(), PLUGIN, Picture, candidates).unwrap();
        assert_eq!(replaced.rejected, [AssetError::UnsafeDimensions]);
        assert_eq!(
            replaced
                .stored
                .iter()
                .map(|stored| stored.source.width)
                .collect::<Vec<_>>(),
            [300, 301, 302, 303]
        );
        assert_eq!(keys(private.path()).len(), 4);
        assert_eq!(
            file(private.path())["wl4:@korri:steamgriddb:picture:3"]["asset_id"],
            asset_id(&images[3], "png")
        );
        // An answer where nothing passes writes nothing: the earlier images
        // stay.
        let replaced = repo
            .replace(owner(), PLUGIN, Picture, vec![candidate(png(64, 64))])
            .unwrap();
        assert_eq!(replaced.stored, []);
        assert_eq!(replaced.rejected, [AssetError::UnsafeDimensions]);
        assert_eq!(keys(private.path()).len(), 4);
    }

    #[test]
    fn media_lists_every_stored_image_of_every_kind_from_every_plugin() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let mut other = owner();
        other.playable_id = "anguna".into();
        let mut bare = owner();
        bare.playable_id = "no-art".into();
        let owners = [owner(), other.clone(), bare];
        assert!(repo.matching_media(&owners).unwrap().is_empty());

        let hero = test_image(1920, 620, ImageFormat::Png, false, [30, 160, 90]);
        let logo = test_image(400, 130, ImageFormat::Png, true, [255, 255, 255]);
        let icon = png(64, 64);
        put(
            &repo,
            &owner(),
            PLUGIN,
            Picture,
            &[png(512, 512), hero.clone()],
        );
        put(&repo, &owner(), PLUGIN, Logo, std::slice::from_ref(&logo));
        put(&repo, &owner(), OTHER, Icon, std::slice::from_ref(&icon));
        put(&repo, &other, PLUGIN, Logo, std::slice::from_ref(&logo));

        let media = repo.matching_media(&owners).unwrap();
        assert_eq!(media.keys().collect::<Vec<_>>(), vec!["anguna", "wl4"]);
        // Key order: plugin id, then kind, then place.
        assert_eq!(
            media["wl4"]
                .iter()
                .map(|stored| (stored.kind, stored.width, stored.height))
                .collect::<Vec<_>>(),
            vec![
                (Icon, 64, 64),
                (Logo, 400, 130),
                (Picture, 512, 512),
                (Picture, 1920, 620),
            ]
        );
        assert_eq!(
            media["wl4"][3],
            StoredMedia {
                kind: Picture,
                asset_id: asset_id(&hero, "png"),
                width: 1920,
                height: 620,
                color: Some("#1ea05a".into()),
            }
        );
        // A white logo has no clear colour.
        assert_eq!(media["anguna"].len(), 1);
        assert_eq!(media["anguna"][0].color, None);
        // An image whose blob is gone is left out.
        fs::remove_file(
            private
                .path()
                .join(STATE_DIR)
                .join(BLOBS_DIR)
                .join(asset_id(&hero, "png")),
        )
        .unwrap();
        assert_eq!(
            repo.matching_media(&owners).unwrap()["wl4"]
                .iter()
                .map(|stored| stored.kind)
                .collect::<Vec<_>>(),
            vec![Icon, Logo, Picture]
        );
    }

    #[test]
    fn the_cover_is_the_largest_square_ish_picture() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        assert_eq!(repo.matching_cover_asset_id(&owner()).unwrap(), None);
        // No square-ish picture, and an icon and a logo that are square: no
        // cover.
        put(
            &repo,
            &owner(),
            PLUGIN,
            Picture,
            &[png(600, 900), png(1920, 620)],
        );
        put(&repo, &owner(), PLUGIN, Icon, &[png(1024, 1024)]);
        put(&repo, &owner(), PLUGIN, Logo, &[png(1024, 1024)]);
        assert_eq!(repo.matching_cover_asset_id(&owner()).unwrap(), None);
        // 0.85 and 1.2 wide for each unit of height are square-ish.
        let low = png(340, 400);
        let high = png(480, 400);
        put(&repo, &owner(), OTHER, Picture, &[low.clone()]);
        assert_eq!(
            repo.matching_cover_asset_id(&owner()).unwrap(),
            Some(asset_id(&low, "png"))
        );
        put(&repo, &owner(), OTHER, Picture, &[high.clone()]);
        assert_eq!(
            repo.matching_cover_asset_id(&owner()).unwrap(),
            Some(asset_id(&high, "png"))
        );
        // The largest wins, from any plugin; of the same area, the first in
        // key order.
        let small = png(512, 512);
        let large = png(1024, 1024);
        let same = png(1024, 1025);
        put(&repo, &owner(), PLUGIN, Picture, &[small, large.clone()]);
        assert_eq!(
            repo.matching_cover_asset_id(&owner()).unwrap(),
            Some(asset_id(&large, "png"))
        );
        put(&repo, &owner(), OTHER, Picture, &[same.clone()]);
        assert_eq!(
            repo.matching_cover_asset_id(&owner()).unwrap(),
            Some(asset_id(&same, "png"))
        );
        // An image whose blob is gone is not a cover.
        fs::remove_file(
            private
                .path()
                .join(STATE_DIR)
                .join(BLOBS_DIR)
                .join(asset_id(&same, "png")),
        )
        .unwrap();
        assert_eq!(
            repo.matching_cover_asset_id(&owner()).unwrap(),
            Some(asset_id(&large, "png"))
        );
        for (width, height, square) in [
            (85, 100, true),
            (84, 100, false),
            (120, 100, true),
            (121, 100, false),
            (2, 3, false),
        ] {
            assert_eq!(square_ish(width, height), square, "{width}x{height}");
        }
    }

    #[test]
    fn the_colour_is_computed_once_and_kept_with_the_assignment() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let stored = put(
            &repo,
            &owner(),
            PLUGIN,
            Picture,
            &[
                test_image(600, 900, ImageFormat::Png, false, [200, 60, 40]),
                test_image(1920, 620, ImageFormat::Png, false, [128, 128, 128]),
            ],
        );
        assert_eq!(stored[0].color.as_deref(), Some("#c83c28"));
        let file = file(private.path());
        assert_eq!(file["wl4:@korri:steamgriddb:picture:0"]["color"], "#c83c28");
        // An image with no clear colour keeps no field.
        assert!(file["wl4:@korri:steamgriddb:picture:1"]
            .get("color")
            .is_none());
    }

    /// An image painted in vertical bands, widest first.
    fn banded(width: u32, height: u32, bands: &[([u8; 4], u32)]) -> image::DynamicImage {
        let total: u32 = bands.iter().map(|(_, share)| share).sum();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(width, height, |x, _| {
            let mut edge = 0;
            for (colour, share) in bands {
                edge += share * width / total;
                if x < edge {
                    return image::Rgba(*colour);
                }
            }
            image::Rgba(bands[bands.len() - 1].0)
        }))
    }

    #[test]
    fn the_colour_is_the_average_of_the_largest_hue_bucket() {
        let flat = |rgb: [u8; 3]| {
            dominant_color(&image::DynamicImage::ImageRgb8(
                image::RgbImage::from_pixel(64, 64, image::Rgb(rgb)),
            ))
        };
        // A flat colour is its own answer, in lowercase hex.
        assert_eq!(flat([200, 60, 40]).as_deref(), Some("#c83c28"));
        assert_eq!(flat([10, 120, 255]).as_deref(), Some("#0a78ff"));
        // Near black, near white and low saturation leave nothing.
        for dull in [
            [0, 0, 0],
            [30, 10, 10],
            [255, 255, 255],
            [240, 235, 250],
            [128, 128, 128],
            [140, 120, 120],
        ] {
            assert_eq!(flat(dull), None, "{dull:?}");
        }
        // The pixels where two bands meet are blended by the reduction, so a
        // banded image is compared within a few steps of each channel.
        let near = |colour: Option<String>, [red, green, blue]: [u8; 3]| {
            let colour = colour.expect("a colour");
            assert!(colour.len() == 7 && colour.starts_with('#'), "{colour}");
            assert_eq!(colour, colour.to_lowercase());
            let channel = |at: usize| u8::from_str_radix(&colour[at..at + 2], 16).unwrap();
            for (found, wanted) in [(channel(1), red), (channel(3), green), (channel(5), blue)] {
                assert!(
                    found.abs_diff(wanted) <= 6,
                    "{colour} is not near {red},{green},{blue}"
                );
            }
        };
        // Two hues: the larger area wins, and it is not mixed with the other.
        let blue = [20, 80, 220, 255];
        let red = [220, 40, 30, 255];
        near(
            dominant_color(&banded(320, 320, &[(blue, 3), (red, 1)])),
            [20, 80, 220],
        );
        near(
            dominant_color(&banded(320, 320, &[(red, 3), (blue, 1)])),
            [220, 40, 30],
        );
        // Black bars, white text and grey do not count, however large.
        let black = [0, 0, 0, 255];
        let white = [255, 255, 255, 255];
        near(
            dominant_color(&banded(320, 320, &[(black, 5), (white, 4), (blue, 1)])),
            [20, 80, 220],
        );
        // A logo: transparent pixels do not count, whatever colour they hide.
        let hidden_red = [220, 40, 30, 0];
        near(
            dominant_color(&banded(320, 320, &[(hidden_red, 3), (blue, 1)])),
            [20, 80, 220],
        );
        assert_eq!(dominant_color(&banded(64, 64, &[(hidden_red, 1)])), None);
        // The same image always gives the same answer, at any size.
        let two = banded(640, 200, &[(blue, 1), (red, 1)]);
        assert_eq!(dominant_color(&two), dominant_color(&two));
        assert!(dominant_color(&banded(4000, 1300, &[(blue, 3), (red, 1)]))
            .is_some_and(|colour| colour.len() == 7 && colour.starts_with('#')));
    }

    #[test]
    fn a_source_without_an_id_or_url_keeps_neither() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let replaced = repo
            .replace(
                owner(),
                PLUGIN,
                Icon,
                vec![AssetCandidate {
                    source_id: None,
                    source_url: None,
                    ..candidate(png(4, 4))
                }],
            )
            .unwrap();
        let source = serde_json::to_value(&replaced.stored[0].source).unwrap();
        assert!(source.get("id").is_none() && source.get("url").is_none());
        assert_eq!(source["provider"], PLUGIN);
    }

    #[test]
    fn records_from_before_the_migration_are_not_read() {
        let asset_id = format!("{}.png", "a".repeat(64));
        let sha256 = format!("sha256:{}", "a".repeat(64));
        // The records of the first artwork migration, and those of the fixed
        // roles, exactly as korrid wrote them.
        for old in [
            serde_json::json!({ "wl4": {
                "asset_id": asset_id, "role": "tile", "owner": owner(),
                "source": { "provider": "steamgriddb", "game_id": 10, "grid_id": 20,
                    "sha256": sha256, "width": 512, "height": 512, "format": "png" }
            } }),
            serde_json::json!({ "wl4:tile": {
                "asset_id": asset_id, "role": "tile", "owner": owner(),
                "source": { "provider": PLUGIN, "id": "20",
                    "sha256": sha256, "width": 512, "height": 512, "format": "png" }
            } }),
            serde_json::json!({ "wl4:@korri:steamgriddb:picture:0": {
                "asset_id": asset_id, "role": "tile", "kind": "picture", "owner": owner(),
                "source": { "provider": PLUGIN, "id": "20",
                    "sha256": sha256, "width": 512, "height": 512, "format": "png" }
            } }),
        ] {
            let private = tempfile::tempdir().unwrap();
            let repo = GameAssetRepository::new(private.path());
            let state = private.path().join(STATE_DIR);
            fs::create_dir_all(&state).unwrap();
            fs::write(state.join(ASSIGNMENTS_FILE), old.to_string()).unwrap();
            // No alias and no fallback read: the file is refused whole, so no
            // new record is written beside the old ones.
            assert_eq!(
                repo.holds_from(&owner(), PLUGIN, None),
                Err(AssetError::Storage)
            );
            assert_eq!(
                repo.replace(owner(), PLUGIN, Picture, vec![candidate(png(512, 512))]),
                Err(AssetError::Storage)
            );
            assert!(repo.matching_cover_asset_ids([&owner()]).is_err());
            assert!(repo.matching_media([&owner()]).is_err());
            assert_eq!(
                fs::read_to_string(state.join(ASSIGNMENTS_FILE)).unwrap(),
                old.to_string()
            );
        }
    }

    /// `deploy/migrate-measured-media.py` turns the first file into the
    /// second; its own test compares against these same two files.
    #[test]
    fn the_one_off_migration_writes_records_that_keep_every_image_and_cover() {
        const BEFORE: &str =
            include_str!("../tests/fixtures/measured-media-migration/before.assignments.json");
        const AFTER: &str =
            include_str!("../tests/fixtures/measured-media-migration/after.assignments.json");
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let state = private.path().join(STATE_DIR);
        fs::create_dir_all(state.join(BLOBS_DIR)).unwrap();
        let old: BTreeMap<String, serde_json::Value> = serde_json::from_str(BEFORE).unwrap();
        for record in old.values() {
            fs::write(
                state
                    .join(BLOBS_DIR)
                    .join(record["asset_id"].as_str().unwrap()),
                PNG_1X1,
            )
            .unwrap();
        }
        let mut owners: Vec<AssetOwnerIdentity> = old
            .values()
            .map(|record| serde_json::from_value(record["owner"].clone()).unwrap())
            .collect();
        owners.dedup();
        assert_eq!(owners.len(), 2);

        // Before the migration korrid reads nothing: no alias, no fallback.
        fs::write(state.join(ASSIGNMENTS_FILE), BEFORE).unwrap();
        assert!(repo.matching_cover_asset_ids(&owners).is_err());

        fs::write(state.join(ASSIGNMENTS_FILE), AFTER).unwrap();
        // Every cover stays: each old tile is its game's cover.
        let covers = repo.matching_cover_asset_ids(&owners).unwrap();
        for (key, record) in &old {
            if record["role"] == "tile" {
                let playable_id = key.strip_suffix(":tile").unwrap();
                assert_eq!(covers[playable_id], record["asset_id"].as_str().unwrap());
            }
        }
        assert_eq!(covers.len(), 2);
        // Every image stays, with its kind, size and colour, from its plugin.
        let media = repo.matching_media(&owners).unwrap();
        let mut listed: Vec<_> = media
            .values()
            .flatten()
            .map(|stored| {
                (
                    stored.asset_id.clone(),
                    stored.kind,
                    stored.width,
                    stored.height,
                    stored.color.clone(),
                )
            })
            .collect();
        listed.sort();
        let mut expected: Vec<_> = old
            .values()
            .map(|record| {
                (
                    record["asset_id"].as_str().unwrap().to_owned(),
                    if record["role"] == "logo" {
                        Logo
                    } else {
                        Picture
                    },
                    record["source"]["width"].as_u64().unwrap() as u32,
                    record["source"]["height"].as_u64().unwrap() as u32,
                    record["color"].as_str().map(str::to_owned),
                )
            })
            .collect();
        expected.sort();
        assert_eq!(listed, expected);
        for owner in &owners {
            assert!(repo.holds_from(owner, PLUGIN, None).unwrap());
            for assignment in repo.assignments(owner).unwrap() {
                assert!(repo.assigned_asset(&assignment.asset_id).unwrap().is_some());
            }
        }
        // A later answer from the plugin replaces the migrated pictures.
        let wario = owners
            .iter()
            .find(|owner| owner.playable_id == "wario-land-4")
            .unwrap();
        put(&repo, wario, PLUGIN, Picture, &[png(512, 512)]);
        assert_eq!(
            repo.assignments(wario)
                .unwrap()
                .iter()
                .filter(|assignment| assignment.kind == Picture)
                .count(),
            1
        );
    }

    #[test]
    fn each_kind_admits_only_its_own_shape() {
        for (kind, width, height, admitted) in [
            // A picture: each side at least 256 pixels, any shape.
            (Picture, 256, 256, true),
            (Picture, 600, 900, true),
            (Picture, 1920, 620, true),
            (Picture, 3840, 1240, true),
            (Picture, 1920, 1080, true),
            (Picture, 4096, 256, true),
            (Picture, 255, 900, false),
            (Picture, 900, 255, false),
            (Picture, 64, 64, false),
            // A logo: any shape.
            (Logo, 400, 120, true),
            (Logo, 120, 400, true),
            (Logo, 1, 1, true),
            // An icon: square.
            (Icon, 1, 1, true),
            (Icon, 256, 256, true),
            (Icon, 256, 255, false),
            (Icon, 600, 900, false),
            // Every kind: at most 4096 pixels a side, and not empty.
            (Icon, 4096, 4096, true),
            (Icon, 4097, 4097, false),
            (Picture, 2732, 4098, false),
            (Picture, 4098, 1366, false),
            (Logo, 4097, 10, false),
            (Logo, 10, 4097, false),
            (Logo, 0, 10, false),
            (Picture, 0, 300, false),
        ] {
            assert_eq!(
                kind_admits_dimensions(kind, width, height),
                admitted,
                "{kind:?} {width}x{height}"
            );
        }
    }

    #[test]
    fn the_shape_rule_is_applied_to_the_real_bytes_of_each_kind() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        for (kind, width, height) in [(Icon, 60, 90), (Picture, 64, 64), (Picture, 255, 1024)] {
            let replaced = repo
                .replace(owner(), PLUGIN, kind, vec![candidate(png(width, height))])
                .unwrap();
            assert_eq!(
                replaced.rejected,
                [AssetError::UnsafeDimensions],
                "{kind:?} {width}x{height}"
            );
        }
        // A JPEG picture and icon are fine: only a logo needs transparency.
        for (kind, width, height) in [(Picture, 600, 900), (Icon, 64, 64)] {
            let jpeg = test_image(width, height, ImageFormat::Jpeg, false, [10, 120, 200]);
            let stored = put(&repo, &owner(), PLUGIN, kind, &[jpeg]);
            assert!(stored[0].asset_id.ends_with(".jpg"));
            assert_eq!(stored[0].source.format, "jpg");
        }
        assert_eq!(repo.assignments(&owner()).unwrap().len(), 2);
    }

    #[test]
    fn a_logo_needs_transparency() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let rgb = [240, 240, 240];
        // An alpha channel in which every pixel is opaque is no transparency.
        let opaque_alpha = |format| {
            let mut bytes = Vec::new();
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                40,
                13,
                image::Rgba([240, 240, 240, 255]),
            ))
            .write_to(&mut Cursor::new(&mut bytes), format)
            .unwrap();
            bytes
        };
        for opaque in [
            test_image(40, 13, ImageFormat::Jpeg, false, rgb),
            test_image(40, 13, ImageFormat::Png, false, rgb),
            test_image(40, 13, ImageFormat::WebP, false, rgb),
            opaque_alpha(ImageFormat::Png),
            opaque_alpha(ImageFormat::WebP),
        ] {
            assert_eq!(
                repo.replace(owner(), PLUGIN, Logo, vec![candidate(opaque)])
                    .unwrap()
                    .rejected,
                [AssetError::Unsupported]
            );
        }
        assert!(!repo.holds_from(&owner(), PLUGIN, None).unwrap());
        for (transparent, extension) in [
            (test_image(40, 13, ImageFormat::Png, true, rgb), ".png"),
            (test_image(40, 13, ImageFormat::WebP, true, rgb), ".webp"),
        ] {
            let stored = put(&repo, &owner(), PLUGIN, Logo, &[transparent]);
            assert!(stored[0].asset_id.ends_with(extension));
        }
    }

    #[test]
    fn serves_only_the_blob_an_assignment_names() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let assignment = repo
            .replace(
                owner(),
                PLUGIN,
                Icon,
                vec![AssetCandidate {
                    declared_width: Some(1),
                    declared_height: Some(1),
                    ..candidate(PNG_1X1.to_vec())
                }],
            )
            .unwrap()
            .stored
            .remove(0);

        assert_eq!(
            repo.assigned_asset(&assignment.asset_id).unwrap(),
            Some(AssignedAsset {
                bytes: PNG_1X1.to_vec(),
                content_type: "image/png",
            })
        );
        assert_eq!(
            asset_url(&assignment.asset_id),
            format!("/game-assets/{}", assignment.asset_id)
        );

        // A well-formed id no assignment names, even with a blob on disk.
        let stray = format!("{}.png", "a".repeat(64));
        fs::write(
            private.path().join(STATE_DIR).join(BLOBS_DIR).join(&stray),
            PNG_1X1,
        )
        .unwrap();
        assert_eq!(repo.assigned_asset(&stray).unwrap(), None);

        // Anything that is not one content address.
        for id in [
            "",
            "assignments.json",
            "../assignments.json",
            "../../@korri:steamgriddb/credential",
            &format!("{}.gif", "a".repeat(64)),
            &format!("{}.png", "A".repeat(64)),
            &format!("{}.png/", "a".repeat(64)),
            &format!("/{}", assignment.asset_id),
        ] {
            assert_eq!(repo.assigned_asset(id).unwrap(), None, "{id}");
        }

        // A later assignment is served too: the records are read again once
        // the file is written again.
        let larger = png(2, 2);
        let later_id = asset_id(&larger, "png");
        assert_eq!(repo.assigned_asset(&later_id).unwrap(), None);
        let mut other = owner();
        other.playable_id = "sm".into();
        let later = put(&repo, &other, PLUGIN, Icon, std::slice::from_ref(&larger));
        assert_eq!(later[0].asset_id, later_id);
        assert_eq!(
            repo.assigned_asset(&later_id)
                .unwrap()
                .map(|asset| asset.bytes),
            Some(larger)
        );
        assert!(repo.assigned_asset(&assignment.asset_id).unwrap().is_some());

        // The assignment stays, the blob is gone.
        fs::remove_file(
            private
                .path()
                .join(STATE_DIR)
                .join(BLOBS_DIR)
                .join(&assignment.asset_id),
        )
        .unwrap();
        assert_eq!(repo.assigned_asset(&assignment.asset_id).unwrap(), None);
    }

    fn png_with_dimensions(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = PNG_1X1.to_vec();
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        let crc = crc32(&bytes[12..29]);
        bytes[29..33].copy_from_slice(&crc.to_be_bytes());
        bytes
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for byte in bytes {
            crc ^= *byte as u32;
            for _ in 0..8 {
                let mask = 0u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    #[test]
    fn rejects_unsafe_header_dimensions_before_full_decode_allocation() {
        for kind in SurfaceMediaKind::ALL {
            assert_eq!(
                validate_image(
                    &candidate(png_with_dimensions(MAX_DIMENSION + 1, MAX_DIMENSION + 1)),
                    kind
                ),
                Err(AssetError::UnsafeDimensions),
                "{kind:?}"
            );
        }
        // A header that claims an admitted picture over almost no data is
        // malformed, not a 19 MiB allocation that succeeds.
        assert!(validate_image(&candidate(png_with_dimensions(3840, 1240)), Picture).is_err());
    }

    #[test]
    fn rejects_unsupported_malformed_and_inconsistent_assets_without_assignment() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        for kind in SurfaceMediaKind::ALL {
            for bytes in [
                b"<svg></svg>".to_vec(),
                b"GIF89a".to_vec(),
                PNG_1X1[..20].to_vec(),
                vec![0; MAX_ASSET_BYTES + 1],
            ] {
                let replaced = repo
                    .replace(
                        owner(),
                        PLUGIN,
                        kind,
                        vec![AssetCandidate {
                            declared_width: Some(1),
                            declared_height: Some(1),
                            ..candidate(bytes)
                        }],
                    )
                    .unwrap();
                assert!(replaced.stored.is_empty() && replaced.rejected.len() == 1);
            }
        }
        assert_eq!(
            repo.replace(
                owner(),
                PLUGIN,
                Icon,
                vec![AssetCandidate {
                    declared_width: Some(2),
                    declared_height: Some(1),
                    ..candidate(PNG_1X1.to_vec())
                }],
            )
            .unwrap()
            .rejected,
            [AssetError::InconsistentDimensions]
        );
        assert!(!repo.holds_from(&owner(), PLUGIN, None).unwrap());
        assert!(!private
            .path()
            .join(STATE_DIR)
            .join(ASSIGNMENTS_FILE)
            .exists());
    }
}
