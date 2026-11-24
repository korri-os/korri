//! Contract 3.8: `$image`, with optional logical-pixel `size` and `pico8` mode.
//!
//! Each loaded surface owns one cache, dropped with the surface, so a switch
//! releases its pixels. Within a surface, images are decoded once per
//! `(url, size, mode)` and kept in least-recently-used order under a byte budget.
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
//!
//! An SVG `data:` URL goes the same way (`svg_data_url`, `render_svg`): it is
//! rendered once, at the size its `width` and `height` give, on the image
//! thread. A surface makes such a picture from its own data (Velvet's stage
//! plates: a few shapes under a large blur), and drawn as a vector it would be
//! rendered again at every size an animation passes through, on the UI thread.

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
pub type Ask = Box<dyn Fn(&str, Option<pico8::Mode>, Option<Size>)>;

/// Contract 3.8: positive whole logical pixels, one decoded pixel per logical pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub fn validate(self) -> Result<Self> {
        if self.width == 0 || self.height == 0 {
            bail!("image size needs positive whole width and height");
        }
        Ok(self)
    }

    /// Keep aspect, cover the box, and never enlarge the stored image.
    fn cover(self, width: u32, height: u32) -> (u32, u32) {
        if self.width >= width || self.height >= height {
            return (width, height);
        }
        let (numerator, denominator) = if u64::from(self.width) * u64::from(height)
            >= u64::from(self.height) * u64::from(width)
        {
            (self.width, width)
        } else {
            (self.height, height)
        };
        let scale =
            |side| (u64::from(side) * u64::from(numerator)).div_ceil(u64::from(denominator)) as u32;
        (scale(width).max(1), scale(height).max(1))
    }
}

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

/// Whether `url` is an SVG `data:` URL: rendered off the UI thread, like a korrid image.
pub fn svg_data_url(url: &str) -> bool {
    url.strip_prefix("data:")
        .is_some_and(|rest| rest.starts_with("image/svg+xml"))
}

/// Render an SVG data URL at its requested cover size (its own size without one),
/// within the shell's limits. Runs on any thread.
pub fn render_svg(url: &str, size: Option<Size>) -> Result<Pixels> {
    let rest = url.strip_prefix("data:").context("not a data URL")?;
    let (header, data) = rest.split_once(',').context("data URL has no comma")?;
    if data.len() > MAX_ENCODED_BYTES * 2 {
        bail!("data URL is too large");
    }
    let bytes = if header.ends_with(";base64") {
        base64::engine::general_purpose::STANDARD.decode(data)?
    } else {
        percent_decode(data)?
    };
    render_svg_bytes(&bytes, size)
}

fn render_svg_bytes(bytes: &[u8], requested: Option<Size>) -> Result<Pixels> {
    let requested = requested.map(Size::validate).transpose()?;
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .map_err(|_| anyhow::anyhow!("SVG does not parse"))?;
    let size = tree.size().to_int_size();
    let (width, height) = (size.width(), size.height());
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        bail!("SVG size {width}x{height} is out of bounds");
    }
    let (draw_width, draw_height) =
        requested.map_or((width, height), |size| size.cover(width, height));
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(draw_width, draw_height).context("no pixmap for the SVG")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            draw_width as f32 / width as f32,
            draw_height as f32 / height as f32,
        ),
        &mut pixmap.as_mut(),
    );
    let mut pixels = Pixels::new(draw_width, draw_height);
    for (out, pixel) in pixels.make_mut_slice().iter_mut().zip(pixmap.pixels()) {
        let colour = pixel.demultiply();
        *out = Rgba8Pixel {
            r: colour.red(),
            g: colour.green(),
            b: colour.blue(),
            a: colour.alpha(),
        };
    }
    Ok(pixels)
}

/// Decode a raster image within the shell's limits. Runs on any thread.
/// Without a requested size, data URLs and package files keep their own size.
pub fn decode_raster(
    bytes: &[u8],
    mode: Option<pico8::Mode>,
    size: Option<Size>,
) -> Result<Pixels> {
    decode(bytes, mode, None, size)
}

/// Decode an image korrid stores (`asset_id`). It can be far larger than any
/// Korri screen. Without a requested size, its longer side over `MAX_STORED_SIDE` is scaled
/// down to that side, keeping its aspect ratio. Runs on the image thread:
/// only the scaled pixels cross to the UI thread and count in the cache.
pub fn decode_stored(
    bytes: &[u8],
    mode: Option<pico8::Mode>,
    size: Option<Size>,
) -> Result<Pixels> {
    decode(bytes, mode, Some(MAX_STORED_SIDE), size)
}

fn decode(
    bytes: &[u8],
    mode: Option<pico8::Mode>,
    longest: Option<u32>,
    size: Option<Size>,
) -> Result<Pixels> {
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
    let dimensions = pixels.dimensions();
    let target = match size {
        Some(size) => Some(size.validate()?.cover(dimensions.0, dimensions.1)),
        None => longest.and_then(|longest| scaled_size(dimensions.0, dimensions.1, longest)),
    };
    if let Some((width, height)) = target.filter(|target| *target != dimensions) {
        // Before the palette step, so that step sees the pixels that are shown.
        pixels = image::imageops::resize(
            &pixels,
            width,
            height,
            if size.is_some() {
                image::imageops::FilterType::Lanczos3
            } else {
                image::imageops::FilterType::Triangle
            },
        );
    }
    if let Some(mode) = mode {
        pico8::quantize(&mut pixels, mode);
    }
    let (width, height) = pixels.dimensions();
    Ok(Pixels::clone_from_slice(pixels.as_raw(), width, height))
}

/// The size of a `width` by `height` image scaled down so that its longer
/// side is `longest`, with its aspect ratio kept. `None` when it already
/// fits: an image is never scaled up.
fn scaled_size(width: u32, height: u32, longest: u32) -> Option<(u32, u32)> {
    let longer = width.max(height);
    if longer <= longest {
        return None;
    }
    let scale = |side: u32| {
        let scaled =
            (u64::from(side) * u64::from(longest) + u64::from(longer) / 2) / u64::from(longer);
        (scaled as u32).max(1)
    };
    Some((scale(width), scale(height)))
}

/// Decoded pixels a surface may keep cached. A window of 200 covers at
/// 128x128 is about 13 MiB.
pub const CACHE_BYTES: usize = 24 * 1024 * 1024;
/// Largest encoded image accepted, data URL or file.
pub const MAX_ENCODED_BYTES: usize = 8 * 1024 * 1024;
/// Largest decoded side, in pixels.
pub const MAX_SIDE: u32 = 4096;
/// Default stored-image cap when the surface gives no draw size. At this bound
/// a 16:9 image is about 8 MiB decoded, inside `CACHE_BYTES`; unscaled, one stored 3611x1877
/// logo is 25.9 MiB, more than the whole budget.
pub const MAX_STORED_SIDE: u32 = 1920;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    url: String,
    mode: Option<pico8::Mode>,
    size: Option<Size>,
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

    pub fn get(
        &mut self,
        url: &str,
        mode: Option<pico8::Mode>,
        size: Option<Size>,
    ) -> Result<Image> {
        let size = size.map(Size::validate).transpose()?;
        let key = Key {
            url: url.to_owned(),
            mode,
            size,
        };
        if let Some((image, _)) = self.entries.get(&key) {
            let image = image.clone();
            self.order.retain(|k| k != &key);
            self.order.push_back(key);
            return Ok(image);
        }
        if svg_data_url(url) && mode.is_some() {
            bail!("pico8 needs a raster image, not SVG");
        }
        if asset_id(url).is_some() || svg_data_url(url) {
            if !self.failed.contains(&key) && !self.waiting.contains(&key) {
                if let Some(ask) = &self.ask {
                    ask(url, mode, size);
                    self.waiting.insert(key);
                }
            }
            return Ok(Image::default());
        }
        let (image, bytes) = self.decode(url, mode, size)?;
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
        size: Option<Size>,
        pixels: Option<Pixels>,
    ) -> bool {
        let key = Key {
            url: url.to_owned(),
            mode,
            size,
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

    fn decode(
        &self,
        url: &str,
        mode: Option<pico8::Mode>,
        size: Option<Size>,
    ) -> Result<(Image, usize)> {
        let (bytes, svg) = self.encoded(url)?;
        if bytes.len() > MAX_ENCODED_BYTES {
            bail!("image is too large");
        }
        if svg {
            if mode.is_some() {
                bail!("pico8 needs a raster image, not SVG");
            }
            if let Some(size) = size {
                let pixels = render_svg_bytes(&bytes, Some(size))?;
                let bytes = (pixels.width() * pixels.height() * 4) as usize;
                return Ok((Image::from_rgba8(pixels), bytes));
            }
            let image = Image::load_from_data(&bytes, Some("svg"))
                .map_err(|_| anyhow::anyhow!("SVG does not decode"))?;
            let size = image.size();
            return Ok((image, (size.width * size.height * 4) as usize));
        }
        let pixels = decode_raster(&bytes, mode, size)?;
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
    fn requested_decode_covers_the_box_keeps_aspect_and_never_upscales() -> Result<()> {
        let bytes = large_png(600, 900);
        for (size, expected) in [
            (
                Size {
                    width: 216,
                    height: 324,
                },
                (216, 324),
            ),
            (
                Size {
                    width: 200,
                    height: 200,
                },
                (200, 300),
            ),
            (
                Size {
                    width: 1000,
                    height: 1000,
                },
                (600, 900),
            ),
            (
                Size {
                    width: 1,
                    height: 1,
                },
                (1, 2),
            ),
        ] {
            for pixels in [
                decode_raster(&bytes, None, Some(size))?,
                decode_stored(&bytes, None, Some(size))?,
            ] {
                assert_eq!((pixels.width(), pixels.height()), expected);
            }
        }
        // Explicit sizes replace the legacy stored-image cap, not the stored size.
        let pixels = decode_stored(
            &large_png(3000, 1500),
            None,
            Some(Size {
                width: 2400,
                height: 1200,
            }),
        )?;
        assert_eq!((pixels.width(), pixels.height()), (2400, 1200));
        assert!(decode_stored(
            &bytes,
            None,
            Some(Size {
                width: 0,
                height: 1
            })
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn cache_identity_includes_url_size_and_palette_and_counts_decoded_pixels() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        let url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(large_png(2, 2))
        );
        let size = Some(Size {
            width: 1,
            height: 1,
        });
        assert_eq!(cache.get(&url, None, size)?.size().width, 1);
        assert_eq!(cache.get(&url, None, size)?.size().width, 1);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.bytes(), 4);
        cache.get(&url, Some(pico8::Mode::Flat), size)?;
        cache.get(&url, None, None)?;
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.bytes(), 4 + 4 + 16);
        cache.load_assets_with(Box::new(|_, _, _| {}));
        let url = asset_url('a');
        cache.get(&url, None, size)?;
        cache.get(&url, None, None)?;
        cache.get(&url, Some(pico8::Mode::Vivid), size)?;
        assert_eq!(cache.waiting(), 3);
        assert!(cache.arrived(
            &url,
            None,
            size,
            Some(decode_raster(&large_png(2, 2), None, size)?)
        ));
        assert_eq!(cache.waiting(), 2);
        assert_eq!(cache.get(&url, None, size)?.size().width, 1);
        assert_eq!(cache.get(&url, None, None)?.size().width, 0);
        Ok(())
    }

    #[test]
    fn twenty_one_visible_posters_fit_the_cache_at_full_hd_and_4k() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let bytes = large_png(600, 900);
        for size in [
            Size {
                width: 216,
                height: 324,
            },
            Size {
                width: 434,
                height: 651,
            },
        ] {
            let mut cache = ImageCache::new(dir.path());
            cache.load_assets_with(Box::new(|_, _, _| {}));
            for at in 0..21 {
                let url = format!("/game-assets/{at:064x}.png");
                cache.get(&url, None, Some(size))?;
                assert!(cache.arrived(
                    &url,
                    None,
                    Some(size),
                    Some(decode_stored(&bytes, None, Some(size))?)
                ));
            }
            assert_eq!(cache.len(), 21);
            assert_eq!(cache.bytes(), (size.width * size.height * 4 * 21) as usize);
            assert!(cache.bytes() <= CACHE_BYTES);
        }
        Ok(())
    }

    #[test]
    fn requested_svg_is_rasterized_at_the_cover_size() -> Result<()> {
        let url = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='600' height='900'%3E%3Crect width='600' height='900' fill='red'/%3E%3C/svg%3E";
        let pixels = render_svg(
            url,
            Some(Size {
                width: 216,
                height: 324,
            }),
        )?;
        assert_eq!((pixels.width(), pixels.height()), (216, 324));
        assert_eq!(pixels.as_slice()[0].r, 255);
        assert!(render_svg(
            url,
            Some(Size {
                width: 0,
                height: 1
            })
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn decodes_once_per_url_and_mode() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        let url = png_data_url();
        let first = cache.get(&url, None, None)?;
        assert_eq!(first, cache.get(&url, None, None)?);
        assert_eq!(cache.len(), 1);
        cache.get(&url, Some(pico8::Mode::Flat), None)?;
        assert_eq!(cache.len(), 2);
        Ok(())
    }

    #[test]
    fn pico8_mode_quantises_the_pixels() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        let image = cache.get(&png_data_url(), Some(pico8::Mode::Flat), None)?;
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
        cache.get(&url, None, None)?;
        cache.get(&url, Some(pico8::Mode::Vivid), None)?;
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
        assert!(cache.get("http://127.0.0.1/a.png", None, None).is_err());
        assert!(cache.get("/etc/passwd", None, None).is_err());
        assert!(cache.get("../secret.png", None, None).is_err());
        assert!(cache.get("data:image/png;base64,!!!", None, None).is_err());
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
        decode_raster(&bytes, None, None).expect("decode")
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
        cache.load_assets_with(Box::new(move |url, mode, _| {
            log.borrow_mut().push((url.to_owned(), mode))
        }));
        let url = asset_url('a');

        assert_eq!(cache.get(&url, None, None)?.size().width, 0);
        assert_eq!(cache.get(&url, None, None)?.size().width, 0);
        assert_eq!(*asked.borrow(), vec![(url.clone(), None)]);
        assert_eq!(cache.waiting(), 1);

        assert!(cache.arrived(&url, None, None, Some(pixels())));
        assert_eq!(cache.waiting(), 0);
        assert_eq!(cache.get(&url, None, None)?.size().width, 2);
        assert_eq!(
            asked.borrow().len(),
            1,
            "a cached image is not asked for again"
        );

        // Pixels nobody asked this cache for (a retired surface's) are dropped.
        assert!(!cache.arrived(&asset_url('b'), None, None, Some(pixels())));
        assert_eq!(cache.len(), 1);
        Ok(())
    }

    #[test]
    fn a_korrid_image_that_fails_stays_empty_and_is_not_asked_for_again() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = asked.clone();
        cache.load_assets_with(Box::new(move |_, _, _| count.set(count.get() + 1)));
        let url = asset_url('a');

        cache.get(&url, None, None)?;
        assert!(!cache.arrived(&url, None, None, None));
        assert_eq!(cache.get(&url, None, None)?.size().width, 0);
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
        cache.load_assets_with(Box::new(move |_, _, _| count.set(count.get() + 1)));
        for url in [
            "/rpc",
            "/game-assets/assignments.json",
            "/game-assets/../rpc",
            "http://127.0.0.1:8099/game-assets/a.png",
        ] {
            assert!(cache.get(url, None, None).is_err(), "{url}");
        }
        assert_eq!(asked.get(), 0);
        assert_eq!(cache.waiting(), 0);
        Ok(())
    }

    #[test]
    fn without_a_loader_a_korrid_image_is_empty_and_no_error() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        assert_eq!(cache.get(&asset_url('a'), None, None)?.size().width, 0);
        assert_eq!(cache.waiting(), 0);
        Ok(())
    }

    /// An encoded PNG of one flat colour: small as bytes, large as pixels.
    fn large_png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(width, height, image::Rgba([0x2b, 0xac, 0xfd, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("encode");
        bytes
    }

    #[test]
    fn a_stored_image_is_scaled_down_to_1920_on_its_longer_side() -> Result<()> {
        assert_eq!(MAX_STORED_SIDE, 1920);
        for ((width, height), (shown_width, shown_height)) in [
            // A hero as SteamGridDB has them: half the size, same shape.
            ((3840, 1240), (1920, 620)),
            // A tall image: the longer side is the height.
            ((1000, 3000), (640, 1920)),
            // The logo that was larger than the whole cache.
            ((3611, 1877), (1920, 998)),
            // Already inside the bound: unchanged, and never scaled up.
            ((600, 900), (600, 900)),
            ((1920, 620), (1920, 620)),
            ((1920, 1920), (1920, 1920)),
            ((2, 1), (2, 1)),
        ] {
            let pixels = decode_stored(&large_png(width, height), None, None)?;
            assert_eq!(
                (pixels.width(), pixels.height()),
                (shown_width, shown_height),
                "{width}x{height}"
            );
            // The picture is the same picture.
            assert_eq!(pixels.as_bytes()[..4], [0x2b, 0xac, 0xfd, 255]);
        }
        // A very thin image keeps at least one pixel of its short side.
        assert_eq!(scaled_size(4096, 1, 1920), Some((1920, 1)));
        assert_eq!(scaled_size(1, 4096, 1920), Some((1, 1920)));
        Ok(())
    }

    #[test]
    fn a_scaled_stored_image_is_quantised_after_it_is_scaled() -> Result<()> {
        let pixels = decode_stored(&large_png(3840, 1240), Some(pico8::Mode::Flat), None)?;
        assert_eq!((pixels.width(), pixels.height()), (1920, 620));
        // The near-blue of `png_data_url`, as the palette step gives it.
        assert_eq!(pixels.as_bytes()[..4], [0x29, 0xad, 0xff, 255]);
        Ok(())
    }

    #[test]
    fn the_cache_counts_the_scaled_size_of_a_stored_image() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut cache = ImageCache::new(dir.path());
        cache.load_assets_with(Box::new(|_, _, _| {}));
        let (hero, logo) = (asset_url('a'), asset_url('b'));
        cache.get(&hero, None, None)?;
        cache.get(&logo, None, None)?;
        // What the image thread hands over for a 3840x1240 hero and for the
        // 3611x1877 logo: together 15 MiB, where the logo alone was 25.9 MiB.
        assert!(cache.arrived(
            &hero,
            None,
            None,
            Some(decode_stored(&large_png(3840, 1240), None, None)?)
        ));
        assert_eq!(cache.bytes(), 1920 * 620 * 4);
        assert!(cache.arrived(
            &logo,
            None,
            None,
            Some(decode_stored(&large_png(3611, 1877), None, None)?)
        ));
        assert_eq!(cache.bytes(), 1920 * 620 * 4 + 1920 * 998 * 4);
        assert!(cache.bytes() < CACHE_BYTES);
        // Both stay cached: neither pushed the other out.
        assert_eq!(cache.len(), 2);
        let shown = cache.get(&hero, None, None)?.size();
        assert_eq!((shown.width, shown.height), (1920, 620));
        Ok(())
    }

    #[test]
    fn a_data_url_and_a_package_file_keep_their_own_size() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let bytes = large_png(3840, 1240);
        std::fs::write(dir.path().join("hero.png"), &bytes)?;
        let url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        let mut cache = ImageCache::new(dir.path());
        // A package sizes its own images: the bound is for korrid's only.
        let shown = cache.get(&url, None, None)?.size();
        assert_eq!((shown.width, shown.height), (3840, 1240));
        assert_eq!(cache.bytes(), 3840 * 1240 * 4);
        let shown = cache.get("hero.png", None, None)?.size();
        assert_eq!((shown.width, shown.height), (3840, 1240));
        let pixels = decode_raster(&bytes, None, None)?;
        assert_eq!((pixels.width(), pixels.height()), (3840, 1240));
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
        assert_eq!(cache.get("art/a.png", None, None)?.size().width, 2);
        Ok(())
    }
}
