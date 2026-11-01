//! Contract 3.8: `{"$image": url}` and `{"$image": url, "pico8": mode}`.
//!
//! Each loaded surface owns one cache, dropped with the surface, so a switch
//! releases its pixels. Within a surface, images are decoded once per
//! `(url, mode)` and kept in least-recently-used order under a byte budget.
//!
//! Sources: `data:` URLs (base64 or percent-encoded), and paths relative to the
//! package folder that stay inside it. korrid publishes no other image URL
//! today, so other schemes are refused rather than fetched.

use crate::pico8;
use anyhow::{bail, Context, Result};
use base64::Engine as _;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
};

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
        }
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
        let (image, bytes) = self.decode(url, mode)?;
        while self.used + bytes > self.budget {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some((_, size)) = self.entries.remove(&oldest) {
                self.used -= size;
            }
        }
        self.used += bytes;
        self.entries.insert(key.clone(), (image.clone(), bytes));
        self.order.push_back(key);
        Ok(image)
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
        let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
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
        let buffer =
            SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(pixels.as_raw(), width, height);
        Ok((Image::from_rgba8(buffer), (width * height * 4) as usize))
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
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
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
