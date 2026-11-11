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
const MAX_ASSET_BYTES: usize = 5 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AssetAssignment {
    pub asset_id: String,
    pub role: String,
    pub owner: AssetOwnerIdentity,
    pub source: SteamGridDbProvenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AssetOwnerIdentity {
    pub playable_id: String,
    pub release_id: String,
    pub release_fingerprint: String,
    pub rom_identity: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct SteamGridDbProvenance {
    pub provider: String,
    pub game_id: u64,
    pub grid_id: u64,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AssetCandidate {
    pub bytes: Vec<u8>,
    pub declared_width: Option<u32>,
    pub declared_height: Option<u32>,
    pub game_id: u64,
    pub grid_id: u64,
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
    pub(crate) fn has_assignment(&self, playable_id: &str) -> Result<bool, AssetError> {
        Ok(read_assignments(&self.root)?.contains_key(playable_id))
    }

    pub(crate) fn matching_assignment(
        &self,
        owner: &AssetOwnerIdentity,
    ) -> Result<Option<AssetAssignment>, AssetError> {
        Ok(matching_assignment(&read_assignments(&self.root)?, owner).cloned())
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
            let Some(assignment) = matching_assignment(&assignments, owner) else {
                continue;
            };
            if assignment.role != "tile" || !valid_asset_id(&assignment.asset_id) {
                continue;
            }
            if !blob_root.join(&assignment.asset_id).is_file() {
                continue;
            }
            asset_ids.insert(owner.playable_id.clone(), assignment.asset_id.clone());
        }
        Ok(asset_ids)
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

    pub(crate) fn assign_tile(
        &self,
        owner: AssetOwnerIdentity,
        candidate: AssetCandidate,
    ) -> Result<AssetAssignment, AssetError> {
        let decoded = validate_image(&candidate)?;
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
            role: "tile".into(),
            owner,
            source: SteamGridDbProvenance {
                provider: "steamgriddb".into(),
                game_id: candidate.game_id,
                grid_id: candidate.grid_id,
                sha256: format!("sha256:{}", decoded.sha256),
                width: decoded.width,
                height: decoded.height,
                format: decoded.extension,
            },
        };
        let mut assignments = read_assignments(&self.root)?;
        assignments.insert(assignment.owner.playable_id.clone(), assignment.clone());
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
}

fn matching_assignment<'a>(
    assignments: &'a BTreeMap<String, AssetAssignment>,
    owner: &AssetOwnerIdentity,
) -> Option<&'a AssetAssignment> {
    assignments
        .get(&owner.playable_id)
        .filter(|assignment| &assignment.owner == owner)
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

fn validate_image(candidate: &AssetCandidate) -> Result<DecodedImage, AssetError> {
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
    if !safe_square_dimensions(width, height) {
        return Err(AssetError::UnsafeDimensions);
    }
    let mut reader = ImageReader::with_format(Cursor::new(&candidate.bytes), format);
    reader.limits(image_limits());
    let image = reader.decode().map_err(|error| image_decode_error(error))?;
    let (width, height) = image.dimensions();
    if !safe_square_dimensions(width, height) {
        return Err(AssetError::UnsafeDimensions);
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
    })
}

fn image_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_ASSET_BYTES as u64);
    limits
}

fn safe_square_dimensions(width: u32, height: u32) -> bool {
    width != 0
        && height != 0
        && width == height
        && width <= MAX_DIMENSION
        && height <= MAX_DIMENSION
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

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_1X1: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 250, 207, 0, 0, 2, 7,
        1, 2, 154, 28, 49, 113, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    fn owner() -> AssetOwnerIdentity {
        AssetOwnerIdentity {
            playable_id: "wl4".into(),
            release_id: "gba".into(),
            release_fingerprint: "sha256:item".into(),
            rom_identity: "sha256:rom".into(),
        }
    }

    #[test]
    fn assigns_valid_static_square_image_content_addressably() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let assignment = repo
            .assign_tile(
                owner(),
                AssetCandidate {
                    bytes: PNG_1X1.to_vec(),
                    declared_width: Some(1),
                    declared_height: Some(1),
                    game_id: 10,
                    grid_id: 20,
                },
            )
            .unwrap();

        assert_eq!(assignment.role, "tile");
        assert!(assignment.asset_id.ends_with(".png"));
        assert!(repo.has_assignment("wl4").unwrap());
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
    fn serves_only_the_blob_an_assignment_names() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        let assignment = repo
            .assign_tile(
                owner(),
                AssetCandidate {
                    bytes: PNG_1X1.to_vec(),
                    declared_width: Some(1),
                    declared_height: Some(1),
                    game_id: 10,
                    grid_id: 20,
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
            "../../steamgriddb.credential",
            &format!("{}.gif", "a".repeat(64)),
            &format!("{}.png", "A".repeat(64)),
            &format!("{}.png/", "a".repeat(64)),
            &format!("/{}", assignment.asset_id),
        ] {
            assert_eq!(repo.assigned_asset(id).unwrap(), None, "{id}");
        }

        // A later assignment is served too: the records are read again once
        // the file is written again.
        let mut larger = Vec::new();
        image::RgbaImage::new(2, 2)
            .write_to(&mut Cursor::new(&mut larger), ImageFormat::Png)
            .unwrap();
        let later_id = format!("{}.png", hex::encode(Sha256::digest(&larger)));
        assert_eq!(repo.assigned_asset(&later_id).unwrap(), None);
        let mut other = owner();
        other.playable_id = "sm".into();
        let later = repo
            .assign_tile(
                other,
                AssetCandidate {
                    bytes: larger.clone(),
                    declared_width: None,
                    declared_height: None,
                    game_id: 11,
                    grid_id: 21,
                },
            )
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
        let candidate = AssetCandidate {
            bytes: png_with_dimensions(MAX_DIMENSION + 1, MAX_DIMENSION + 1),
            declared_width: None,
            declared_height: None,
            game_id: 10,
            grid_id: 20,
        };

        assert_eq!(
            validate_image(&candidate),
            Err(AssetError::UnsafeDimensions)
        );
    }

    #[test]
    fn rejects_unsupported_malformed_and_inconsistent_assets_without_assignment() {
        let private = tempfile::tempdir().unwrap();
        let repo = GameAssetRepository::new(private.path());
        for bytes in [
            b"<svg></svg>".to_vec(),
            b"GIF89a".to_vec(),
            PNG_1X1[..20].to_vec(),
        ] {
            assert!(repo
                .assign_tile(
                    owner(),
                    AssetCandidate {
                        bytes,
                        declared_width: Some(1),
                        declared_height: Some(1),
                        game_id: 10,
                        grid_id: 20,
                    },
                )
                .is_err());
        }
        assert!(repo
            .assign_tile(
                owner(),
                AssetCandidate {
                    bytes: PNG_1X1.to_vec(),
                    declared_width: Some(2),
                    declared_height: Some(1),
                    game_id: 10,
                    grid_id: 20,
                },
            )
            .is_err());
        assert!(!repo.has_assignment("wl4").unwrap());
    }
}
