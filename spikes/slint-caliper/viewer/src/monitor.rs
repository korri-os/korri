//! Physical pixels per millimetre of the monitor the window is on.
//!
//! Order: the saved calibration for that monitor (Caliper decision 7: one credit-card
//! match per monitor), then the monitor's EDID image size from
//! /sys/class/drm/*-<connector>/edid (rounded to 1 mm, or 1 cm in the base block), then
//! `KORRI_VIEWER_MONITOR_MM=WxH` (for a headless session, which has no EDID), then
//! Caliper's assumption of 96 dpi (DEFAULT_PX_PER_MM) at the window's scale.
//!
//! The saved file is `$XDG_STATE_HOME/korri-slint-caliper/px-per-mm` (default
//! `~/.local/state/…`), one `<monitor> <physical px per mm>` line per monitor. Caliper keeps
//! one value in the browser's localStorage (`caliper:px-per-mm`); this location and form
//! are the spike's own and unresolved.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Measure {
    /// Monitor pixels per millimetre (Slint's `phx` per mm).
    pub px_per_mm: f32,
    pub source: String,
    pub calibrated: bool,
}

pub struct Monitor {
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub scale: f32,
}

pub fn measure(monitor: &Monitor) -> Measure {
    if let Some(saved) = saved(&monitor.name) {
        return Measure {
            px_per_mm: saved,
            source: format!("{}: calibrated", monitor.name),
            calibrated: true,
        };
    }
    if let Some((w, h)) = edid_mm(&monitor.name) {
        return Measure {
            px_per_mm: monitor.width_px as f32 / w,
            source: format!(
                "{}: EDID {w} x {h} mm, {} x {} px, not calibrated",
                monitor.name, monitor.width_px, monitor.height_px
            ),
            calibrated: false,
        };
    }
    if let Some((w, h)) = std::env::var("KORRI_VIEWER_MONITOR_MM").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
    }) {
        return Measure {
            px_per_mm: monitor.width_px as f32 / w,
            source: format!(
                "{}: KORRI_VIEWER_MONITOR_MM {w} x {h} mm, {} x {} px, not calibrated",
                monitor.name, monitor.width_px, monitor.height_px
            ),
            calibrated: false,
        };
    }
    Measure {
        px_per_mm: 96.0 / 25.4 * monitor.scale,
        source: format!("{}: no EDID, 96 dpi assumed, not calibrated", monitor.name),
        calibrated: false,
    }
}

/// The EDID image size in mm: the first detailed timing descriptor, else bytes 21 and 22 (cm).
pub fn edid_mm(connector: &str) -> Option<(f32, f32)> {
    let entries = std::fs::read_dir("/sys/class/drm").ok()?;
    for entry in entries.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if file.split_once('-').map(|(_, c)| c) != Some(connector) {
            continue;
        }
        let edid = std::fs::read(entry.path().join("edid")).ok()?;
        return parse_edid(&edid);
    }
    None
}

pub fn parse_edid(edid: &[u8]) -> Option<(f32, f32)> {
    if edid.len() < 128 || edid[..8] != [0, 255, 255, 255, 255, 255, 255, 0] {
        return None;
    }
    let d = &edid[54..72];
    let pixel_clock = u16::from_le_bytes([d[0], d[1]]);
    if pixel_clock != 0 {
        let w = d[12] as u32 | ((d[14] as u32 >> 4) << 8);
        let h = d[13] as u32 | ((d[14] as u32 & 0xf) << 8);
        if w > 0 && h > 0 {
            return Some((w as f32, h as f32));
        }
    }
    (edid[21] > 0 && edid[22] > 0).then(|| (edid[21] as f32 * 10.0, edid[22] as f32 * 10.0))
}

pub fn store() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
        });
    base.join("korri-slint-caliper/px-per-mm")
}

fn lines() -> Vec<(String, f32)> {
    std::fs::read_to_string(store())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (name, v) = l.rsplit_once(' ')?;
            Some((name.to_string(), v.parse().ok()?))
        })
        .collect()
}

fn saved(monitor: &str) -> Option<f32> {
    lines()
        .into_iter()
        .find(|(n, _)| n == monitor)
        .map(|(_, v)| v)
        .filter(|v| *v > 0.0)
}

/// Save (Some) or forget (None) the calibration of one monitor.
pub fn save(monitor: &str, px_per_mm: Option<f32>) -> std::io::Result<()> {
    let mut all: Vec<(String, f32)> = lines().into_iter().filter(|(n, _)| n != monitor).collect();
    if let Some(v) = px_per_mm {
        all.push((monitor.to_string(), v));
    }
    let path = store();
    std::fs::create_dir_all(path.parent().unwrap())?;
    let text: String = all.iter().map(|(n, v)| format!("{n} {v:.3}\n")).collect();
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    #[test]
    fn edid_detailed_timing_size() {
        // A 340 x 220 mm panel (myoko's eDP-1): 340 = 0x154, 220 = 0x0dc.
        let mut edid = vec![0u8; 128];
        edid[..8].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 0]);
        edid[21] = 34;
        edid[22] = 22;
        edid[54] = 0x10;
        edid[54 + 12] = 0x54;
        edid[54 + 13] = 0xdc;
        edid[54 + 14] = 0x10;
        assert_eq!(super::parse_edid(&edid), Some((340.0, 220.0)));
    }
}
