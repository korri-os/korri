use image::{GenericImageView, ImageFormat, ImageReader, Limits};
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

/// What a stored image of a game is for: legacy `GameAssetRole` without
/// `banner` and `screenshot`, which have no consumer (ADR 0032 as widened
/// for Velvet). `tile` is the square cover (`LocalGame.coverAssetId`).
#[typeshare::typeshare(serialized_as = "GameAssetRoleWire")]
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GameAssetRole {
    Tile,
    Poster,
    Hero,
    Logo,
}

impl GameAssetRole {
    pub(crate) const ALL: [Self; 4] = [Self::Tile, Self::Poster, Self::Hero, Self::Logo];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Tile => "tile",
            Self::Poster => "poster",
            Self::Hero => "hero",
            Self::Logo => "logo",
        }
    }
}

/// One image of one game, in `assignments.json` under `<playable id>:<role>`
/// (legacy `GameAssetAssignmentRecord.id`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AssetAssignment {
    pub asset_id: String,
    pub role: GameAssetRole,
    pub owner: AssetOwnerIdentity,
    pub source: GameAssetSource,
    /// One colour of this image as lowercase `#rrggbb`, computed once when
    /// the image is stored (`dominant_color`). Absent when the image has no
    /// clear colour, and on a record stored before colours were computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A stored `poster`, `hero` or `logo` of one game, as the game list
/// carries it. The `tile` is the cover and is never in this list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredMedia {
    pub role: GameAssetRole,
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

/// Downloaded bytes a plugin proposed for one role, with what it said of
/// them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AssetCandidate {
    pub bytes: Vec<u8>,
    pub declared_width: Option<u32>,
    pub declared_height: Option<u32>,
    /// The owning plugin's id. A plugin cannot claim another source.
    pub provider: String,
    pub source_id: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
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

    #[cfg(test)]
    pub(crate) fn has_assignment(
        &self,
        playable_id: &str,
        role: GameAssetRole,
    ) -> Result<bool, AssetError> {
        Ok(read_assignments(&self.root)?.contains_key(&assignment_key(playable_id, role)))
    }

    /// The roles this exact game already has an image for.
    pub(crate) fn assigned_roles(
        &self,
        owner: &AssetOwnerIdentity,
    ) -> Result<BTreeSet<GameAssetRole>, AssetError> {
        let assignments = read_assignments(&self.root)?;
        Ok(GameAssetRole::ALL
            .into_iter()
            .filter(|role| matching_assignment(&assignments, owner, *role).is_some())
            .collect())
    }

    #[cfg(test)]
    pub(crate) fn matching_assignment(
        &self,
        owner: &AssetOwnerIdentity,
        role: GameAssetRole,
    ) -> Result<Option<AssetAssignment>, AssetError> {
        Ok(matching_assignment(&read_assignments(&self.root)?, owner, role).cloned())
    }

    #[cfg(test)]
    pub(crate) fn matching_tile_asset_id(
        &self,
        owner: &AssetOwnerIdentity,
    ) -> Result<Option<String>, AssetError> {
        Ok(self
            .matching_tile_asset_ids(std::iter::once(owner))?
            .remove(&owner.playable_id))
    }

    pub(crate) fn matching_tile_asset_ids<'a>(
        &self,
        owners: impl IntoIterator<Item = &'a AssetOwnerIdentity>,
    ) -> Result<BTreeMap<String, String>, AssetError> {
        let assignments = read_assignments(&self.root)?;
        let blob_root = self.root.join(BLOBS_DIR);
        let mut asset_ids = BTreeMap::new();
        for owner in owners {
            let Some(assignment) = matching_assignment(&assignments, owner, GameAssetRole::Tile)
            else {
                continue;
            };
            if !valid_asset_id(&assignment.asset_id) {
                continue;
            }
            if !blob_root.join(&assignment.asset_id).is_file() {
                continue;
            }
            asset_ids.insert(owner.playable_id.clone(), assignment.asset_id.clone());
        }
        Ok(asset_ids)
    }

    /// The stored `poster`, `hero` and `logo` of each of these exact games,
    /// in that order, by playable id. A game with none is not in the map. An
    /// image whose blob is gone is left out, as for the tile.
    pub(crate) fn matching_media<'a>(
        &self,
        owners: impl IntoIterator<Item = &'a AssetOwnerIdentity>,
    ) -> Result<BTreeMap<String, Vec<StoredMedia>>, AssetError> {
        const MEDIA_ROLES: [GameAssetRole; 3] = [
            GameAssetRole::Poster,
            GameAssetRole::Hero,
            GameAssetRole::Logo,
        ];
        let assignments = read_assignments(&self.root)?;
        let blob_root = self.root.join(BLOBS_DIR);
        let mut media = BTreeMap::new();
        for owner in owners {
            let stored: Vec<StoredMedia> = MEDIA_ROLES
                .into_iter()
                .filter_map(|role| matching_assignment(&assignments, owner, role))
                .filter(|assignment| {
                    valid_asset_id(&assignment.asset_id)
                        && blob_root.join(&assignment.asset_id).is_file()
                })
                .map(|assignment| StoredMedia {
                    role: assignment.role,
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

    /// Check the bytes against the role's rules, then keep them as this
    /// game's image for that role, replacing an earlier one.
    pub(crate) fn assign(
        &self,
        owner: AssetOwnerIdentity,
        role: GameAssetRole,
        candidate: AssetCandidate,
    ) -> Result<AssetAssignment, AssetError> {
        let decoded = validate_image(&candidate, role)?;
        fs::create_dir_all(self.root.join(BLOBS_DIR)).map_err(|_| AssetError::Storage)?;
        let blob_name = format!("{}.{}", decoded.sha256, decoded.extension);
        let blob_path = self.root.join(BLOBS_DIR).join(&blob_name);
        let wrote_blob = if !blob_path.exists() {
            write_atomically(&blob_path, &candidate.bytes)?;
            true
        } else {
            false
        };
        let assignment = AssetAssignment {
            asset_id: blob_name,
            role,
            owner,
            source: GameAssetSource {
                provider: candidate.provider,
                id: candidate.source_id,
                url: candidate.source_url,
                sha256: format!("sha256:{}", decoded.sha256),
                width: decoded.width,
                height: decoded.height,
                format: decoded.extension,
            },
            color: decoded.color,
        };
        let mut assignments = read_assignments(&self.root)?;
        assignments.insert(
            assignment_key(&assignment.owner.playable_id, role),
            assignment.clone(),
        );
        if let Err(error) = write_json_atomically(&self.root.join(ASSIGNMENTS_FILE), &assignments) {
            if wrote_blob {
                let _ = fs::remove_file(&blob_path);
            }
            return Err(error);
        }
        Ok(assignment)
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

/// `<playable id>:<role>`, the key of one assignment.
fn assignment_key(playable_id: &str, role: GameAssetRole) -> String {
    format!("{playable_id}:{}", role.as_str())
}

fn matching_assignment<'a>(
    assignments: &'a BTreeMap<String, AssetAssignment>,
    owner: &AssetOwnerIdentity,
    role: GameAssetRole,
) -> Option<&'a AssetAssignment> {
    assignments
        .get(&assignment_key(&owner.playable_id, role))
        .filter(|assignment| assignment.role == role && &assignment.owner == owner)
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
    role: GameAssetRole,
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
        .map_err(|error| image_dimension_error(error))?;
    if !role_admits_dimensions(role, width, height) {
        return Err(AssetError::UnsafeDimensions);
    }
    // A logo lies over other art, so it needs an alpha channel, which only
    // PNG and WebP carry.
    if role == GameAssetRole::Logo && format == ImageFormat::Jpeg {
        return Err(AssetError::Unsupported);
    }
    let mut reader = ImageReader::with_format(Cursor::new(&candidate.bytes), format);
    reader.limits(image_limits());
    let image = reader.decode().map_err(|error| image_decode_error(error))?;
    let (width, height) = image.dimensions();
    if !role_admits_dimensions(role, width, height) {
        return Err(AssetError::UnsafeDimensions);
    }
    if role == GameAssetRole::Logo && !image.color().has_alpha() {
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

/// The size rule of each role. Every image is at most 4096 pixels a side.
/// - `tile`: square.
/// - `poster`: width to height is 2 to 3, within 2 % (SteamGridDB's
///   `600x900`; the legacy importer's `2x3`).
/// - `hero`: wider than tall, from 2 to 1 up to 4 to 1 (observed heroes are
///   `1920x620` and `3840x1240`).
/// - `logo`: any shape.
fn role_admits_dimensions(role: GameAssetRole, width: u32, height: u32) -> bool {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return false;
    }
    let (width, height) = (u64::from(width), u64::from(height));
    match role {
        GameAssetRole::Tile => width == height,
        // |w/h - 2/3| <= 2 % of 2/3, in whole numbers: |150w - 100h| <= 2h.
        GameAssetRole::Poster => (150 * width).abs_diff(100 * height) <= 2 * height,
        GameAssetRole::Hero => width >= 2 * height && width <= 4 * height,
        GameAssetRole::Logo => true,
    }
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
/// pass: real bytes of a real format, with or without an alpha channel.
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
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([r, g, b, 255]),
        ))
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

    const PNG_1X1: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 250, 207, 0, 0, 2, 7,
        1, 2, 154, 28, 49, 113, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    const PLUGIN: &str = "@korri:steamgriddb";

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
            provider: PLUGIN.into(),
            source_id: Some("20".into()),
            source_url: Some("https://cdn2.steamgriddb.com/grid/20.png".into()),
        }
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        test_image(width, height, ImageFormat::Png, true, [200, 40, 40])
    }

    #[test]
    fn assigns_valid_static_square_image_content_addressably() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let assignment = repo
            .assign(
                owner(),
                GameAssetRole::Tile,
                AssetCandidate {
                    declared_width: Some(1),
                    declared_height: Some(1),
                    ..candidate(PNG_1X1.to_vec())
                },
            )
            .unwrap();

        assert_eq!(assignment.role, GameAssetRole::Tile);
        assert!(assignment.asset_id.ends_with(".png"));
        assert!(repo.has_assignment("wl4", GameAssetRole::Tile).unwrap());
        assert_eq!(
            repo.matching_tile_asset_id(&owner()).unwrap(),
            Some(assignment.asset_id.clone())
        );

        fs::remove_file(
            private
                .path()
                .join(STATE_DIR)
                .join(BLOBS_DIR)
                .join(&assignment.asset_id),
        )
        .unwrap();
        assert_eq!(repo.matching_tile_asset_id(&owner()).unwrap(), None);
    }

    #[test]
    fn each_role_has_its_own_assignment_under_playable_id_and_role() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let images = [
            (GameAssetRole::Tile, png(8, 8)),
            (GameAssetRole::Poster, png(60, 90)),
            (GameAssetRole::Hero, png(192, 62)),
            (GameAssetRole::Logo, png(40, 13)),
        ];
        for (role, bytes) in &images {
            repo.assign(owner(), *role, candidate(bytes.clone()))
                .unwrap();
        }
        assert_eq!(
            repo.assigned_roles(&owner()).unwrap(),
            GameAssetRole::ALL.into_iter().collect()
        );
        // The file: one record per key, the role in the key and in the record,
        // and the source as legacy `GameAssetSource` plus what korrid measured.
        let file: serde_json::Value = serde_json::from_slice(
            &fs::read(private.path().join(STATE_DIR).join(ASSIGNMENTS_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            file.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["wl4:hero", "wl4:logo", "wl4:poster", "wl4:tile"]
        );
        let hero = &file["wl4:hero"];
        assert_eq!(hero["role"], "hero");
        assert_eq!(hero["owner"]["playable_id"], "wl4");
        let sha256 = hex::encode(Sha256::digest(&images[2].1));
        assert_eq!(hero["asset_id"], format!("{sha256}.png"));
        assert_eq!(
            hero["source"],
            serde_json::json!({
                "provider": PLUGIN,
                "id": "20",
                "url": "https://cdn2.steamgriddb.com/grid/20.png",
                "sha256": format!("sha256:{sha256}"),
                "width": 192,
                "height": 62,
                "format": "png",
            })
        );
        // The tile alone is the cover id.
        assert_eq!(
            repo.matching_tile_asset_id(&owner()).unwrap(),
            Some(format!("{}.png", hex::encode(Sha256::digest(&images[0].1))))
        );
        // A new image for one role replaces only that role's record.
        let replacement = png(120, 40);
        repo.assign(owner(), GameAssetRole::Hero, candidate(replacement.clone()))
            .unwrap();
        assert_eq!(
            repo.matching_assignment(&owner(), GameAssetRole::Hero)
                .unwrap()
                .unwrap()
                .asset_id,
            format!("{}.png", hex::encode(Sha256::digest(&replacement)))
        );
        assert_eq!(repo.assigned_roles(&owner()).unwrap().len(), 4);
        // A game whose identity changed has no image under the old records.
        let mut changed = owner();
        changed.rom_identity = "sha256:other-rom".into();
        assert!(repo.assigned_roles(&changed).unwrap().is_empty());
        assert_eq!(repo.matching_tile_asset_id(&changed).unwrap(), None);
    }

    #[test]
    fn media_lists_exactly_the_stored_poster_hero_and_logo_and_never_the_tile() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let mut other = owner();
        other.playable_id = "anguna".into();
        let mut bare = owner();
        bare.playable_id = "no-art".into();
        let owners = [owner(), other.clone(), bare];
        assert!(repo.matching_media(&owners).unwrap().is_empty());

        // wl4: every role. anguna: a tile and a logo only.
        let hero = test_image(192, 62, ImageFormat::Png, false, [30, 160, 90]);
        let logo = test_image(40, 13, ImageFormat::Png, true, [255, 255, 255]);
        for (role, bytes) in [
            (GameAssetRole::Logo, logo.clone()),
            (GameAssetRole::Hero, hero.clone()),
            (GameAssetRole::Tile, png(8, 8)),
            (GameAssetRole::Poster, png(60, 90)),
        ] {
            repo.assign(owner(), role, candidate(bytes)).unwrap();
        }
        repo.assign(other.clone(), GameAssetRole::Tile, candidate(png(8, 8)))
            .unwrap();
        repo.assign(other, GameAssetRole::Logo, candidate(logo.clone()))
            .unwrap();

        let media = repo.matching_media(&owners).unwrap();
        assert_eq!(media.keys().collect::<Vec<_>>(), vec!["anguna", "wl4"]);
        // Poster, hero, logo, whatever order they were stored in.
        assert_eq!(
            media["wl4"]
                .iter()
                .map(|stored| (stored.role, stored.width, stored.height))
                .collect::<Vec<_>>(),
            vec![
                (GameAssetRole::Poster, 60, 90),
                (GameAssetRole::Hero, 192, 62),
                (GameAssetRole::Logo, 40, 13)
            ]
        );
        assert_eq!(
            media["wl4"][1],
            StoredMedia {
                role: GameAssetRole::Hero,
                asset_id: format!("{}.png", hex::encode(Sha256::digest(&hero))),
                width: 192,
                height: 62,
                color: Some("#1ea05a".into()),
            }
        );
        // A game with only some roles lists exactly those.
        assert_eq!(
            media["anguna"]
                .iter()
                .map(|stored| stored.role)
                .collect::<Vec<_>>(),
            vec![GameAssetRole::Logo]
        );
        // A white logo has no clear colour.
        assert_eq!(media["anguna"][0].color, None);
        // The tile is the cover, in its own list.
        assert_eq!(repo.matching_tile_asset_ids(&owners).unwrap().len(), 2);
        // An image whose blob is gone is left out.
        fs::remove_file(
            private
                .path()
                .join(STATE_DIR)
                .join(BLOBS_DIR)
                .join(&media["wl4"][1].asset_id),
        )
        .unwrap();
        assert_eq!(
            repo.matching_media(&owners).unwrap()["wl4"]
                .iter()
                .map(|stored| stored.role)
                .collect::<Vec<_>>(),
            vec![GameAssetRole::Poster, GameAssetRole::Logo]
        );
    }

    #[test]
    fn the_colour_is_computed_once_and_kept_with_the_assignment() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let assignment = repo
            .assign(
                owner(),
                GameAssetRole::Poster,
                candidate(test_image(60, 90, ImageFormat::Png, false, [200, 60, 40])),
            )
            .unwrap();
        assert_eq!(assignment.color.as_deref(), Some("#c83c28"));
        let file: serde_json::Value = serde_json::from_slice(
            &fs::read(private.path().join(STATE_DIR).join(ASSIGNMENTS_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(file["wl4:poster"]["color"], "#c83c28");
        // An image with no clear colour keeps no field.
        repo.assign(
            owner(),
            GameAssetRole::Hero,
            candidate(test_image(
                192,
                62,
                ImageFormat::Png,
                false,
                [128, 128, 128],
            )),
        )
        .unwrap();
        let file: serde_json::Value = serde_json::from_slice(
            &fs::read(private.path().join(STATE_DIR).join(ASSIGNMENTS_FILE)).unwrap(),
        )
        .unwrap();
        assert!(file["wl4:hero"].get("color").is_none());
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
        let assignment = repo
            .assign(
                owner(),
                GameAssetRole::Tile,
                AssetCandidate {
                    source_id: None,
                    source_url: None,
                    ..candidate(png(4, 4))
                },
            )
            .unwrap();
        let source = serde_json::to_value(&assignment.source).unwrap();
        assert!(source.get("id").is_none() && source.get("url").is_none());
        assert_eq!(source["provider"], PLUGIN);
    }

    #[test]
    fn records_from_before_the_migration_are_not_read() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let state = private.path().join(STATE_DIR);
        fs::create_dir_all(&state).unwrap();
        let asset_id = format!("{}.png", "a".repeat(64));
        // The old key and the old `source`, exactly as main wrote them.
        fs::write(
            state.join(ASSIGNMENTS_FILE),
            serde_json::json!({ "wl4": {
                "asset_id": asset_id, "role": "tile", "owner": owner(),
                "source": { "provider": "steamgriddb", "game_id": 10, "grid_id": 20,
                    "sha256": format!("sha256:{}", "a".repeat(64)), "width": 512, "height": 512, "format": "png" }
            } })
            .to_string(),
        )
        .unwrap();
        // No alias and no fallback read: the file is refused whole, so no new
        // record is written beside the old ones.
        assert_eq!(repo.assigned_roles(&owner()), Err(AssetError::Storage));
        assert_eq!(
            repo.assign(owner(), GameAssetRole::Tile, candidate(png(4, 4))),
            Err(AssetError::Storage)
        );
        assert!(repo
            .matching_tile_asset_ids(std::iter::once(&owner()))
            .is_err());
    }

    /// `deploy/migrate-plugin-artwork.py` turns the first file into the
    /// second; its own test compares against these same two files.
    #[test]
    fn the_one_off_migration_writes_records_that_keep_every_cover() {
        const BEFORE: &str =
            include_str!("../tests/fixtures/artwork-migration/before.assignments.json");
        const AFTER: &str =
            include_str!("../tests/fixtures/artwork-migration/after.assignments.json");
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
        let owners: Vec<AssetOwnerIdentity> = old
            .values()
            .map(|record| serde_json::from_value(record["owner"].clone()).unwrap())
            .collect();

        // Before the migration korrid reads nothing: no alias, no fallback.
        fs::write(state.join(ASSIGNMENTS_FILE), BEFORE).unwrap();
        assert!(repo.matching_tile_asset_ids(&owners).is_err());

        fs::write(state.join(ASSIGNMENTS_FILE), AFTER).unwrap();
        let covers = repo.matching_tile_asset_ids(&owners).unwrap();
        assert_eq!(covers.len(), old.len());
        for (playable_id, record) in &old {
            let asset_id = record["asset_id"].as_str().unwrap();
            assert_eq!(covers[playable_id], asset_id);
            assert!(repo.assigned_asset(asset_id).unwrap().is_some());
        }
        for owner in &owners {
            assert_eq!(
                repo.assigned_roles(owner).unwrap(),
                [GameAssetRole::Tile].into_iter().collect()
            );
            let source = repo
                .matching_assignment(owner, GameAssetRole::Tile)
                .unwrap()
                .unwrap()
                .source;
            assert_eq!(source.provider, PLUGIN);
            assert!(source.id.is_some() && source.url.is_none());
        }
    }

    #[test]
    fn each_role_admits_only_its_own_shape() {
        use GameAssetRole::{Hero, Logo, Poster, Tile};
        for (role, width, height, admitted) in [
            (Tile, 512, 512, true),
            (Tile, 512, 511, false),
            (Tile, 600, 900, false),
            // 2 to 3, within 2 %.
            (Poster, 600, 900, true),
            (Poster, 1000, 1500, true),
            (Poster, 612, 900, true),
            (Poster, 588, 900, true),
            (Poster, 613, 900, false),
            (Poster, 587, 900, false),
            (Poster, 900, 600, false),
            (Poster, 512, 512, false),
            // Wider than tall, from 2 to 1 up to 4 to 1.
            (Hero, 1920, 620, true),
            (Hero, 3840, 1240, true),
            (Hero, 1240, 620, true),
            (Hero, 2480, 620, true),
            (Hero, 1239, 620, false),
            (Hero, 2481, 620, false),
            (Hero, 620, 1920, false),
            (Hero, 512, 512, false),
            // Any shape.
            (Logo, 400, 120, true),
            (Logo, 120, 400, true),
            (Logo, 512, 512, true),
            // Every role: at most 4096 pixels a side, and not empty.
            (Tile, 4096, 4096, true),
            (Tile, 4097, 4097, false),
            (Poster, 2732, 4098, false),
            (Hero, 4098, 1366, false),
            (Logo, 4097, 10, false),
            (Logo, 10, 4097, false),
            (Logo, 0, 10, false),
        ] {
            assert_eq!(
                role_admits_dimensions(role, width, height),
                admitted,
                "{role:?} {width}x{height}"
            );
        }
    }

    #[test]
    fn the_shape_rule_is_applied_to_the_real_bytes_of_each_role() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        for (role, width, height) in [
            (GameAssetRole::Tile, 60, 90),
            (GameAssetRole::Poster, 64, 64),
            (GameAssetRole::Hero, 64, 64),
            (GameAssetRole::Hero, 60, 90),
        ] {
            assert_eq!(
                repo.assign(owner(), role, candidate(png(width, height))),
                Err(AssetError::UnsafeDimensions),
                "{role:?} {width}x{height}"
            );
        }
        // A JPEG poster and hero are fine: only a logo needs alpha.
        for (role, width, height) in [
            (GameAssetRole::Poster, 60, 90),
            (GameAssetRole::Hero, 192, 62),
        ] {
            let jpeg = test_image(width, height, ImageFormat::Jpeg, false, [10, 120, 200]);
            let assignment = repo.assign(owner(), role, candidate(jpeg)).unwrap();
            assert!(assignment.asset_id.ends_with(".jpg"));
            assert_eq!(assignment.source.format, "jpg");
        }
        assert!(repo.assigned_roles(&owner()).unwrap().len() == 2);
    }

    #[test]
    fn a_logo_needs_an_alpha_channel() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let rgb = [240, 240, 240];
        for opaque in [
            test_image(40, 13, ImageFormat::Jpeg, false, rgb),
            test_image(40, 13, ImageFormat::Png, false, rgb),
            test_image(40, 13, ImageFormat::WebP, false, rgb),
        ] {
            assert_eq!(
                repo.assign(owner(), GameAssetRole::Logo, candidate(opaque)),
                Err(AssetError::Unsupported)
            );
        }
        assert!(!repo.has_assignment("wl4", GameAssetRole::Logo).unwrap());
        for (transparent, extension) in [
            (test_image(40, 13, ImageFormat::Png, true, rgb), ".png"),
            (test_image(40, 13, ImageFormat::WebP, true, rgb), ".webp"),
        ] {
            let assignment = repo
                .assign(owner(), GameAssetRole::Logo, candidate(transparent))
                .unwrap();
            assert!(assignment.asset_id.ends_with(extension));
        }
    }

    #[test]
    fn serves_only_the_blob_an_assignment_names() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let assignment = repo
            .assign(
                owner(),
                GameAssetRole::Tile,
                AssetCandidate {
                    declared_width: Some(1),
                    declared_height: Some(1),
                    ..candidate(PNG_1X1.to_vec())
                },
            )
            .unwrap();

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
        let later_id = format!("{}.png", hex::encode(Sha256::digest(&larger)));
        assert_eq!(repo.assigned_asset(&later_id).unwrap(), None);
        let mut other = owner();
        other.playable_id = "sm".into();
        let later = repo
            .assign(other, GameAssetRole::Tile, candidate(larger.clone()))
            .unwrap();
        assert_eq!(later.asset_id, later_id);
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
        for role in GameAssetRole::ALL {
            assert_eq!(
                validate_image(
                    &candidate(png_with_dimensions(MAX_DIMENSION + 1, MAX_DIMENSION + 1)),
                    role
                ),
                Err(AssetError::UnsafeDimensions),
                "{role:?}"
            );
        }
        // A header that claims an admitted hero over almost no data is
        // malformed, not a 19 MiB allocation that succeeds.
        assert!(validate_image(
            &candidate(png_with_dimensions(3840, 1240)),
            GameAssetRole::Hero
        )
        .is_err());
    }

    #[test]
    fn rejects_unsupported_malformed_and_inconsistent_assets_without_assignment() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        for role in GameAssetRole::ALL {
            for bytes in [
                b"<svg></svg>".to_vec(),
                b"GIF89a".to_vec(),
                PNG_1X1[..20].to_vec(),
                vec![0; MAX_ASSET_BYTES + 1],
            ] {
                assert!(repo
                    .assign(
                        owner(),
                        role,
                        AssetCandidate {
                            declared_width: Some(1),
                            declared_height: Some(1),
                            ..candidate(bytes)
                        },
                    )
                    .is_err());
            }
        }
        assert_eq!(
            repo.assign(
                owner(),
                GameAssetRole::Tile,
                AssetCandidate {
                    declared_width: Some(2),
                    declared_height: Some(1),
                    ..candidate(PNG_1X1.to_vec())
                },
            ),
            Err(AssetError::InconsistentDimensions)
        );
        assert!(repo.assigned_roles(&owner()).unwrap().is_empty());
    }
}
