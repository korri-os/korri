//! The ground's drift (surfaces/zone/src/plan/ground.ts), Modern look.
//!
//! The rings never move, so they are painted once, here, into two pixel
//! buffers: the ground with ring B (opaque), and ring A alone over its own
//! bounding box (with alpha), so ring A covers ring B's dot as Zone's
//! canvas does. The dots are the only moving part; the window draws them.

use slint::{Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer};

/// A ring's thickness in art px (ZONE_RING_THICKNESS).
const THICKNESS: f32 = 6.0;

#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub cx: f32,
    pub cy: f32,
    pub r: f32,
    /// 1 turns clockwise, -1 the other way.
    pub turn: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Dot {
    pub r: f32,
    pub phase: f32,
}

pub struct Rings {
    pub a: Ring,
    pub a_dots: [Dot; 2],
    pub b: Ring,
    pub b_dots: [Dot; 1],
}

/// `zoneGroundRings(wArt, hArt)`, in art px.
pub fn rings(w_art: f32, h_art: f32) -> Rings {
    let short = w_art.min(h_art);
    Rings {
        a: Ring {
            cx: (w_art * 0.5).round(),
            cy: (h_art * 0.56).round(),
            r: (short * 0.46).round(),
            turn: 1.0,
        },
        a_dots: [
            Dot {
                r: 10.0,
                phase: 0.75,
            },
            Dot {
                r: 6.0,
                phase: 0.08,
            },
        ],
        b: Ring {
            cx: (w_art * 0.8).round(),
            cy: (h_art * 0.36).round(),
            r: (short * 0.34).round(),
            turn: -1.0,
        },
        b_dots: [Dot { r: 8.0, phase: 0.5 }],
    }
}

fn rgb(c: u32) -> [f32; 3] {
    [
        ((c >> 16) & 0xff) as f32,
        ((c >> 8) & 0xff) as f32,
        (c & 0xff) as f32,
    ]
}

/// How much of the pixel centred at (x, y) the annulus covers: an
/// analytic one-pixel ramp at each edge, as an antialiased arc looks.
fn coverage(x: f32, y: f32, cx: f32, cy: f32, outer: f32, inner: f32) -> f32 {
    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    let out = (outer - d + 0.5).clamp(0.0, 1.0);
    let inn = (inner - d + 0.5).clamp(0.0, 1.0);
    (out - inn).max(0.0)
}

/// The ground with ring B, `w` by `h` physical px; `scale` px an art px.
pub fn paint_ground(
    w: u32,
    h: u32,
    scale: f32,
    ring: Ring,
    ground: u32,
    colour: u32,
) -> SharedPixelBuffer<Rgb8Pixel> {
    let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(w.max(1), h.max(1));
    let g = rgb(ground);
    let c = rgb(colour);
    let (cx, cy) = (ring.cx * scale, ring.cy * scale);
    let half = THICKNESS / 2.0 * scale;
    let (outer, inner) = (ring.r * scale + half, (ring.r * scale - half).max(0.0));
    let stride = w.max(1) as usize;
    for (i, px) in buffer.make_mut_slice().iter_mut().enumerate() {
        let x = (i % stride) as f32 + 0.5;
        let y = (i / stride) as f32 + 0.5;
        let k = coverage(x, y, cx, cy, outer, inner);
        let mix = |n: usize| (g[n] + (c[n] - g[n]) * k).round() as u8;
        *px = Rgb8Pixel {
            r: mix(0),
            g: mix(1),
            b: mix(2),
        };
    }
    buffer
}

/// Ring A alone, over its bounding box. Returns the buffer and the box's
/// top left in physical px.
pub fn paint_ring(
    scale: f32,
    ring: Ring,
    colour: u32,
) -> (SharedPixelBuffer<Rgba8Pixel>, f32, f32) {
    let half = THICKNESS / 2.0 * scale;
    let (cx, cy) = (ring.cx * scale, ring.cy * scale);
    let (outer, inner) = (ring.r * scale + half, (ring.r * scale - half).max(0.0));
    let x0 = (cx - outer - 1.0).floor();
    let y0 = (cy - outer - 1.0).floor();
    let side = (2.0 * outer + 3.0).ceil() as u32;
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(side.max(1), side.max(1));
    let c = rgb(colour);
    let stride = side.max(1) as usize;
    for (i, px) in buffer.make_mut_slice().iter_mut().enumerate() {
        let x = x0 + (i % stride) as f32 + 0.5;
        let y = y0 + (i / stride) as f32 + 0.5;
        let k = coverage(x, y, cx, cy, outer, inner);
        // Premultiplication is Slint's job; this is straight alpha.
        *px = Rgba8Pixel {
            r: c[0] as u8,
            g: c[1] as u8,
            b: c[2] as u8,
            a: (k * 255.0).round() as u8,
        };
    }
    (buffer, x0, y0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_match_zone_at_640x480() {
        let r = rings(320.0, 240.0);
        assert_eq!((r.a.cx, r.a.cy, r.a.r), (160.0, 134.0, 110.0));
        assert_eq!((r.b.cx, r.b.cy, r.b.r), (256.0, 86.0, 82.0));
    }

    #[test]
    fn coverage_ramps_at_the_edges() {
        assert_eq!(coverage(0.0, 0.0, 0.0, 0.0, 10.0, 5.0), 0.0);
        assert_eq!(coverage(7.5, 0.0, 0.0, 0.0, 10.0, 5.0), 1.0);
        assert!((coverage(10.0, 0.0, 0.0, 0.0, 10.0, 5.0) - 0.5).abs() < 1e-6);
    }
}
