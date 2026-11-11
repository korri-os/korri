//! Contract 3.8: `{"$image": url}` and `{"$image": url, "pico8": mode}`.
//!
//! Each loaded surface owns one cache, dropped with the surface, so a switch
//! releases its pixels. Within a surface, images are decoded once per
//! `(url, mode)` and kept in least-recently-used order under a byte budget.
//!
//! Sources: `data:` URLs (base64 or percent-encoded), paths relative to the
//! package folder that stay inside it, and the name of an image korrid stores
//! (`asset_id`, read with `app.game-assets.object`). Every other URL is
//! refused.
//!
//! A `data:` URL or a package file is decoded where the view is applied. A
//! korrid image is not: it is fetched and decoded on another thread (`Ask`),
//! so `get` answers an empty image at once and `arrived` takes the pixels
//! later. The shell then converts again the view values that name it. A
//! korrid image that cannot be loaded stays empty; it never fails a view.

use crate::pico8;
use anyhow::{bail, Context, Result};
use base64::Engine as _;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
};

/// Decoded pixels, as they cross from the loading thread to the UI thread.
pub type Pixels = SharedPixelBuffer<Rgba8Pixel>;

/// Pixels on their way to the UI thread. Printed as their size only.
pub struct Arrival(pub Pixels);

impl std::fmt::Debug for Arrival {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Arrival({}x{})", self.0.width(), self.0.height())
    }
}

/// Asks for a korrid image (`asset_id`) to be loaded off the UI thread.
/// The answer comes back through `ImageCache::arrived`.
pub type Ask = Box<dyn Fn(&str, Option<pico8::Mode>)>;

/// The asset id of `url`, when `url` names an image korrid stores as the
/// model does (`SurfaceGame.coverArtUrl`): `/game-assets/` and one content
/// address, `<sha256>.<png|jpg|webp>`, and nothing else. The shell asks its
/// korrid for that id alone, so a model can never make it ask for anything
/// else.
pub fn asset_id(url: &str) -> Option<&str> {
    let id = url.strip_prefix("/game-assets/")?;
    let (hash, extension) = id.split_once('.')?;
    (hash.len() == 64
        && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && matches!(extension, "png" | "jpg" | "webp"))
    .then_some(id)
}

/// Decode a raster image within the shell's limits. Runs on any thread.
pub fn decode_raster(bytes: &[u8], mode: Option<pico8::Mode>) -> Result<Pixels> {
    if bytes.len() > MAX_ENCODED_BYTES {
        bail!("image is too large");
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_SIDE as u64 * MAX_SIDE as u64 * 4);
    reader.limits(limits);
    let mut pixels = reader.decode()?.into_rgba8();
    if let Some(mode) = mode {
        pico8::quantize(&mut pixels, mode);
    }
    let (width, height) = pixels.dimensions();
    Ok(Pixels::clone_from_slice(pixels.as_raw(), width, height))
}

/// Decoded pixels a surface may keep cached. A window of 200 covers at
/// 128x128 is about 13 MiB.
pub const CACHE_BYTES: usize = 24 * 1024 * 1024;
/// Largest encoded image accepted, data URL or file.
pub const MAX_ENCODED_BYTES: usize = 8 * 1024 * 1024;
/// Largest decoded side, in pixels.
pub const MAX_SIDE: u32 = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    url: String,
    mode: Option<pico8::Mode>,
}

pub struct ImageCache {
    package: PathBuf,
    budget: usize,
    used: usize,
    entries: HashMap<Key, (Image, usize)>,
    order: VecDeque<Key>,
    /// Loads korrid images. Without one (the verifier, previews, benches)
    /// a korrid image is empty and nothing is fetched.
    ask: Option<Ask>,
    /// Asked for, not yet arrived.
    waiting: HashSet<Key>,
    /// Could not be loaded: empty for the life of this cache, not asked again.
    failed: HashSet<Key>,
}

impl ImageCache {
    pub fn new(package: &Path) -> Self {
        Self::with_budget(package, CACHE_BYTES)
    }

    pub fn with_budget(package: &Path, budget: usize) -> Self {
        Self {
            package: package.to_owned(),
            budget,
            used: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
            ask: None,
            waiting: HashSet::new(),
            failed: HashSet::new(),
        }
    }

    /// Load korrid images through `ask`.
    pub fn load_assets_with(&mut self, ask: Ask) {
        self.ask = Some(ask);
    }

    /// How many korrid images were asked for and have not arrived.
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.used
    }

    pub fn get(&mut self, url: &str, mode: Option<pico8::Mode>) -> Result<Image> {
        let key = Key {
            url: url.to_owned(),
            mode,
        };
        if let Some((image, _)) = self.entries.get(&key) {
            let image = image.clone();
            self.order.retain(|k| k != &key);
            self.order.push_back(key);
            return Ok(image);
        }
        if asset_id(url).is_some() {
            if !self.failed.contains(&key) && !self.waiting.contains(&key) {
                if let Some(ask) = &self.ask {
                    ask(url, mode);
                    self.waiting.insert(key);
                }
            }
            return Ok(Image::default());
        }
        let (image, bytes) = self.decode(url, mode)?;
        self.keep(key, image.clone(), bytes);
        Ok(image)
    }

    /// The answer to an `Ask`: the pixels, or `None` when the image could not
    /// be loaded. True when the image is now cached, so the views that name
    /// it are worth converting again.
    pub fn arrived(
        &mut self,
        url: &str,
        mode: Option<pico8::Mode>,
        pixels: Option<Pixels>,
    ) -> bool {
        let key = Key {
            url: url.to_owned(),
            mode,
        };
        if !self.waiting.remove(&key) {
            return false; // not asked for by this cache
        }
        let Some(pixels) = pixels else {
            self.failed.insert(key);
            return false;
        };
        let bytes = (pixels.width() * pixels.height() * 4) as usize;
        self.keep(key, Image::from_rgba8(pixels), bytes);
        true
    }

    fn keep(&mut self, key: Key, image: Image, bytes: usize) {
        while self.used + bytes > self.budget {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some((_, size)) = self.entries.remove(&oldest) {
                self.used -= size;
            }
        }
        self.used += bytes;
        self.entries.insert(key.clone(), (image, bytes));
        self.order.push_back(key);
    }

    fn encoded(&self, url: &str) -> Result<(Vec<u8>, bool)> {
        if let Some(rest) = url.strip_prefix("data:") {
            let (header, data) = rest.split_once(',').context("data URL has no comma")?;
            if data.len() > MAX_ENCODED_BYTES * 2 {
                bail!("data URL is too large");
            }
            let svg = header.starts_with("image/svg+xml");
            let bytes = if header.ends_with(";base64") {
                base64::engine::general_purpose::STANDARD.decode(data)?
            } else {
                percent_decode(data)?
            };
            return Ok((bytes, svg));
        }
        if url.contains("://") || url.starts_with('/') {
            bail!("only data: URLs and package-relative paths are images");
        }
        let root = self.package.canonicalize()?;
        let path = root
            .join(url)
            .canonicalize()
            .with_context(|| format!("image file {url}"))?;
        if !path.starts_with(&root) {
            bail!("image path leaves the package: {url}");
        }
        let metadata = std::fs::metadata(&path)?;
        if metadata.len() > MAX_ENCODED_BYTES as u64 {
            bail!("image file is too large: {url}");
        }
        let svg = path.extension().is_some_and(|ext| ext == "svg");
        Ok((std::fs::read(path)?, svg))
    }

    fn decode(&self, url: &str, mode: Option<pico8::Mode>) -> Result<(Image, usize)> {
        let (bytes, svg) = self.encoded(url)?;
        if bytes.len() > MAX_ENCODED_BYTES {
            bail!("image is too large");
        }
        if svg {
            if mode.is_some() {
                bail!("pico8 needs a raster image, not SVG");
            }
            let image = Image::load_from_data(&bytes, Some("svg"))
                .map_err(|_| anyhow::anyhow!("SVG does not decode"))?;
            let size = image.size();
            return Ok((image, (size.width * size.height * 4) as usize));
        }
        let pixels = decode_raster(&bytes, mode)?;
        let size = (pixels.width() * pixels.height() * 4) as usize;
        Ok((Image::from_rgba8(pixels), size))
    }
}

fn percent_decode(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next().context("short percent escape")?;
            let low = bytes.next().context("short percent escape")?;
            let hex = [high, low];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex)?, 16)?);
        } else {
            out.push(byte);
        }
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 2x1 PNG: one near-blue pixel, one transparent.
    pub fn png_data_url() -> String {
        let mut bytes = Vec::new();
        let image = image::RgbaImage::from_raw(2, 1, vec![0x2b, 0xac, 0xfd, 255, 9, 9, 9, 0])
            .expect("pixels");
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("encode");
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }

    #[test]
    fn decodes_once_per_url_and_mode() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        let url = png_data_url();
        let first = cache.get(&url, None)?;
        assert_eq!(first, cache.get(&url, None)?);
        assert_eq!(cache.len(), 1);
        cache.get(&url, Some(pico8::Mode::Flat))?;
        assert_eq!(cache.len(), 2);
        Ok(())
    }

    #[test]
    fn pico8_mode_quantises_the_pixels() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        let image = cache.get(&png_data_url(), Some(pico8::Mode::Flat))?;
        let buffer = image.to_rgba8().expect("rgba image");
        let pixels: Vec<u8> = buffer.as_bytes().to_vec();
        assert_eq!(&pixels[..4], &[0x29, 0xad, 0xff, 255]);
        assert_eq!(pixels[7], 0, "transparent pixel keeps its alpha");
        Ok(())
    }

    #[test]
    fn evicts_least_recently_used_beyond_the_budget() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::with_budget(dir.path(), 8);
        let url = png_data_url();
        cache.get(&url, None)?;
        cache.get(&url, Some(pico8::Mode::Vivid))?;
        assert_eq!(cache.len(), 1);
        assert!(cache.bytes() <= 8);
        Ok(())
    }

    #[test]
    fn refuses_remote_and_escaping_paths() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir(dir.path().join("pkg"))?;
        std::fs::write(dir.path().join("secret.png"), b"x")?;
        let mut cache = ImageCache::new(&dir.path().join("pkg"));
        assert!(cache.get("http://127.0.0.1/a.png", None).is_err());
        assert!(cache.get("/etc/passwd", None).is_err());
        assert!(cache.get("../secret.png", None).is_err());
        assert!(cache.get("data:image/png;base64,!!!", None).is_err());
        Ok(())
    }

    fn asset_url(fill: char) -> String {
        format!("/game-assets/{}.png", fill.to_string().repeat(64))
    }

    fn pixels() -> Pixels {
        let url = png_data_url();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(url.split_once(',').expect("comma").1)
            .expect("base64");
        decode_raster(&bytes, None).expect("decode")
    }

    #[test]
    fn only_a_content_address_under_game_assets_is_a_korrid_image() {
        let good = asset_url('a');
        assert_eq!(asset_id(&good), Some(&good["/game-assets/".len()..]));
        assert!(asset_id(&good.replace(".png", ".jpg")).is_some());
        assert!(asset_id(&good.replace(".png", ".webp")).is_some());
        for bad in [
            "/game-assets/".to_owned(),
            "/game-assets/assignments.json".to_owned(),
            good.replace(".png", ".gif"),
            good.replace(".png", ".png/"),
            good.replace(".png", ".png?x=1"),
            good.replace('a', "A"),
            good.replace("/game-assets/", "/game-assets/../"),
            good.replace("/game-assets/", "/rpc/"),
            format!("http://127.0.0.1:1{good}"),
            format!("{}a.png", &good[..good.len() - 4]),
            good[..good.len() - 5].to_owned() + ".png",
        ] {
            assert_eq!(asset_id(&bad), None, "{bad}");
        }
    }

    #[test]
    fn a_korrid_image_is_empty_until_it_arrives_and_is_asked_for_once() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let log = asked.clone();
        cache.load_assets_with(Box::new(move |url, mode| {
            log.borrow_mut().push((url.to_owned(), mode))
        }));
        let url = asset_url('a');

        assert_eq!(cache.get(&url, None)?.size().width, 0);
        assert_eq!(cache.get(&url, None)?.size().width, 0);
        assert_eq!(*asked.borrow(), vec![(url.clone(), None)]);
        assert_eq!(cache.waiting(), 1);

        assert!(cache.arrived(&url, None, Some(pixels())));
        assert_eq!(cache.waiting(), 0);
        assert_eq!(cache.get(&url, None)?.size().width, 2);
        assert_eq!(
            asked.borrow().len(),
            1,
            "a cached image is not asked for again"
        );

        // Pixels nobody asked this cache for (a retired surface's) are dropped.
        assert!(!cache.arrived(&asset_url('b'), None, Some(pixels())));
        assert_eq!(cache.len(), 1);
        Ok(())
    }

    #[test]
    fn a_korrid_image_that_fails_stays_empty_and_is_not_asked_for_again() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = asked.clone();
        cache.load_assets_with(Box::new(move |_, _| count.set(count.get() + 1)));
        let url = asset_url('a');

        cache.get(&url, None)?;
        assert!(!cache.arrived(&url, None, None));
        assert_eq!(cache.get(&url, None)?.size().width, 0);
        assert_eq!(asked.get(), 1);
        assert!(cache.is_empty());
        Ok(())
    }

    #[test]
    fn a_path_that_is_not_a_stored_image_is_refused_and_never_asked_for() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = asked.clone();
        cache.load_assets_with(Box::new(move |_, _| count.set(count.get() + 1)));
        for url in [
            "/rpc",
            "/game-assets/assignments.json",
            "/game-assets/../rpc",
            "http://127.0.0.1:8099/game-assets/a.png",
        ] {
            assert!(cache.get(url, None).is_err(), "{url}");
        }
        assert_eq!(asked.get(), 0);
        assert_eq!(cache.waiting(), 0);
        Ok(())
    }

    #[test]
    fn without_a_loader_a_korrid_image_is_empty_and_no_error() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        assert_eq!(cache.get(&asset_url('a'), None)?.size().width, 0);
        assert_eq!(cache.waiting(), 0);
        Ok(())
    }

    #[test]
    fn reads_package_relative_files() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir(dir.path().join("art"))?;
        let url = png_data_url();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(url.split_once(',').expect("comma").1)?;
        std::fs::write(dir.path().join("art/a.png"), bytes)?;
        let mut cache = ImageCache::new(dir.path());
        assert_eq!(cache.get("art/a.png", None)?.size().width, 2);
        Ok(())
    }
}
