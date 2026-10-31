//! THROWAWAY SPIKE: the GAMES page (state Ready, Modern look) as a pure
//! function of the window's width and height.
//!
//! Ported from Zone, only as far as this one state needs:
//!  - the art pixel: `surfaces/zone/src/plan/art-px.ts`;
//!  - the zone select's strips, band and cards: `src/plan/zone-select.ts`
//!    and the band's sums in `src/ui/molecules/ZoneCardBand.css`;
//!  - the top strip's levels: `src/plan/top-strip.ts` (inline, tight and
//!    keys; the two-row `stacked` level is not ported);
//!  - every other length from the token it names in `src/zone-tokens.css`
//!    and the component stylesheets, in art px.
//!
//! Every length here is in logical px (the window's CSS px). `a` is the art
//! pixel in px.

use crate::text::Measure;

/// One drawable thing, in paint order. Mirrors `Prim` in ui/zone.slint.
#[derive(Clone, Debug, Default)]
pub struct Prim {
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: u32,
    pub radius: f32,
    pub path: String,
    pub text: String,
    pub size: f32,
    pub elide: bool,
    pub fx: i32,
    pub r: f32,
    pub ring: u32,
    pub drop: u32,
    pub band: u32,
    pub band_y: f32,
    pub band_h: f32,
    pub art: i32,
    pub ix: f32,
    pub iy: f32,
    pub iw: f32,
    pub ih: f32,
    /// Text only: drawn inside this box alone (a clipped panel's words).
    pub clip: Option<(f32, f32, f32, f32)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Rect,
    Path,
    Text,
    Cover,
}

impl Kind {
    pub fn code(self) -> i32 {
        match self {
            Kind::Rect => 0,
            Kind::Path => 1,
            Kind::Text => 2,
            Kind::Cover => 3,
        }
    }
}

/// A cursor triangle beside the focused card.
#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub edge: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub near: f32,
    pub far: f32,
    pub side: i32,
}

/// Zone's palette (zone-tokens.css), as 0xRRGGBB.
pub mod colour {
    pub const MINT: u32 = 0x4ad39c;
    pub const BLACK: u32 = 0x000000;
    pub const WHITE: u32 = 0xf7f3f7;
    pub const YELLOW: u32 = 0xf7db08;
    pub const HUD_EDGE: u32 = 0x3151ad;
    pub const BLUE: u32 = 0x5a71e7;
    pub const ORANGE: u32 = 0xf78e18;
    pub const RED: u32 = 0xe70000;
    pub const PINK: u32 = 0xce6584;
    pub const SHADE: u32 = 0x2e9e72;
    pub const TRACK: u32 = 0x3a313a;
    pub const BANNER_BLUE: u32 = 0x0020b5;
    pub const BANNER_YELLOW: u32 = 0xf7db00;
    pub const BANNER_WHITE: u32 = 0xe7e3e7;
    pub const GREEN: u32 = 0x18a800;
    pub const KEY_DISC: u32 = 0x292029;
    pub const KEY_B: u32 = 0xef285a;
    pub const KEY_A: u32 = 0x639f3d;
    pub const RING_A: u32 = 0x39b2ce;
    pub const RING_B: u32 = ORANGE;
    pub const STRIPE_LIGHT: u32 = 0x9ce7c6;
    pub const STRIPE_TEAL: u32 = 0x5aa2b5;
}

use colour::*;

/// Five cards at or above this width in art px (ZONE_WIDE_WIDTH).
const WIDE_WIDTH: f32 = 333.0;
/// Tall strips at or above this height in art px (ZONE_TALL_HEIGHT).
const TALL_HEIGHT: f32 = 320.0;
const ART_ROWS: f32 = 240.0;

/// The art pixel: `max(1, floor(min(w, h) / 240))` (art-px.ts).
pub fn art_px(w: f32, h: f32) -> f32 {
    (w.min(h).max(0.0) / ART_ROWS).floor().max(1.0)
}

/// The top strip's level (top-strip.ts), by width in art px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopLevel {
    Inline,
    Tight,
    Keys,
}

pub fn top_level(w_art: f32) -> TopLevel {
    if w_art >= 476.0 {
        TopLevel::Inline
    } else if w_art >= 420.0 {
        TopLevel::Tight
    } else {
        // `keys`; below 312 Zone stacks two rows, not ported (README).
        TopLevel::Keys
    }
}

/// The zone select's plan (zone-select.ts `planArt`), in art px.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub a: f32,
    pub w_art: f32,
    pub h_art: f32,
    pub visible: usize,
    pub tab_h: f32,
    pub act_h: f32,
    pub foot_h: f32,
    pub tri: f32,
    pub gap: f32,
}

pub fn plan(w: f32, h: f32) -> Plan {
    let a = art_px(w, h);
    let w_art = (w / a).floor();
    let h_art = (h / a).floor();
    let wide = w_art >= WIDE_WIDTH;
    let tall = h_art >= TALL_HEIGHT;
    Plan {
        a,
        w_art,
        h_art,
        visible: if wide { 5 } else { 3 },
        tab_h: if tall { 40.0 } else { 36.0 },
        act_h: if tall { 30.0 } else { 26.0 },
        foot_h: if tall { 24.0 } else { 22.0 },
        tri: if wide { 14.0 } else { 12.0 },
        gap: if wide { 10.0 } else { 7.0 },
    }
}

/// One fixture card (src/fixtures/zone-cards.ts, the first four games).
struct Card {
    title: &'static str,
    art: i32,
    /// Width over height of the cover (sample-art.ts).
    art_ratio: f32,
    tags: &'static [(TagKind, &'static str)],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TagKind {
    System,
    Here,
    Remote,
    Continue,
}

const CARDS: [Card; 4] = [
    Card {
        title: "Celeste Classic",
        art: 0,
        // 128 by 128.
        art_ratio: 1.0,
        // Celeste can be continued (zone-games.ts): CONTINUE closes its row.
        tags: &[
            (TagKind::System, "PICO-8"),
            (TagKind::Here, "HERE"),
            (TagKind::Continue, "CONTINUE"),
        ],
    },
    Card {
        title: "Hollow Knight",
        art: 1,
        art_ratio: 85.0 / 128.0,
        tags: &[(TagKind::System, "Switch"), (TagKind::Here, "HERE")],
    },
    Card {
        title: "Tetris",
        art: 2,
        art_ratio: 96.0 / 128.0,
        tags: &[(TagKind::System, "GB"), (TagKind::Remote, "zao")],
    },
    Card {
        title: "Spelunky",
        art: 3,
        art_ratio: 128.0 / 60.0,
        tags: &[
            (TagKind::System, "PC"),
            (TagKind::Here, "HERE"),
            (TagKind::Remote, "zao"),
        ],
    },
];

/// The fixture's focused card: Hollow Knight (`FocusedCard`, index 1).
const FOCUS: usize = 1;

/// How a run of words is inked (ZoneCaps.css `data-ink`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ink {
    /// White face, black ring and one-step drop.
    White,
    /// Yellow face, HUD-blue ring and drop.
    Hud,
    /// White face, one-art-px black ring (a tag).
    Tag,
    /// Flat ink, no edge.
    Flat(u32),
    /// The banner: yellow face, black ring, orange two-step drop, white band.
    Banner,
}

/// Everything the window draws for one size.
pub struct Scene {
    pub prims: Vec<Prim>,
    pub tris: Vec<Tri>,
    pub plan: Plan,
}

struct Builder<'m> {
    m: &'m Measure,
    a: f32,
    prims: Vec<Prim>,
}

impl Builder<'_> {
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: u32) {
        self.prims.push(Prim {
            kind: Kind::Rect,
            x,
            y,
            w,
            h,
            color,
            ..Prim::default()
        });
    }

    fn disc(&mut self, x: f32, y: f32, d: f32, color: u32) {
        self.prims.push(Prim {
            kind: Kind::Rect,
            x,
            y,
            w: d,
            h: d,
            color,
            radius: d / 2.0,
            ..Prim::default()
        });
    }

    /// A filled polygon, its points in screen px.
    fn poly(&mut self, points: &[(f32, f32)], color: u32) {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let mut path = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            path.push_str(if i == 0 { "M " } else { " L " });
            path.push_str(&format!("{x:.3} {y:.3}"));
        }
        path.push_str(" Z");
        self.prims.push(Prim {
            kind: Kind::Path,
            x: x0,
            y: y0,
            w: (x1 - x0).max(0.01),
            h: (y1 - y0).max(0.01),
            color,
            path,
            ..Prim::default()
        });
    }

    /// A parallelogram leaning right by `run` (ZonePanel.css's shape).
    fn panel(&mut self, x: f32, y: f32, w: f32, h: f32, run: f32, color: u32) {
        self.poly(
            &[(x + run, y), (x + w, y), (x + w - run, y + h), (x, y + h)],
            color,
        );
    }

    /// Words, cut to the box of the panel that holds them: Zone clips a
    /// keycap's word and a tag (`clip-path` on the element, not a pseudo),
    /// so their ring and drop end at its edge.
    #[allow(clippy::too_many_arguments)]
    fn words_in(
        &mut self,
        text: &str,
        x: f32,
        cap_top: f32,
        cap: f32,
        ink: Ink,
        clip: (f32, f32, f32, f32),
    ) {
        self.words(text, x, cap_top, cap, ink, None);
        if let Some(last) = self.prims.last_mut() {
            last.clip = Some(clip);
        }
    }

    /// Words whose cap stands at `cap_top`, `cap` px tall, from `x`.
    /// `width` limits them (ellipsis) when given.
    fn words(&mut self, text: &str, x: f32, cap_top: f32, cap: f32, ink: Ink, width: Option<f32>) {
        let size = self.m.font_size(cap);
        let natural = self.m.width(text, cap);
        let (w, elide) = match width {
            Some(limit) if limit < natural => (limit.max(0.0), true),
            _ => (natural + cap, false),
        };
        let h = self.m.line_height(cap);
        let y = cap_top - self.m.above_cap(cap);
        let a = self.a;
        let (color, fx, ring, drop, band) = match ink {
            Ink::White => (WHITE, 2, BLACK, BLACK, 0),
            Ink::Hud => (YELLOW, 2, HUD_EDGE, HUD_EDGE, 0),
            Ink::Tag => (WHITE, 1, BLACK, BLACK, 0),
            Ink::Flat(c) => (c, 0, 0, 0, 0),
            Ink::Banner => (BANNER_YELLOW, 3, BLACK, ORANGE, BANNER_WHITE),
        };
        let above = self.m.above_cap(cap);
        self.prims.push(Prim {
            kind: Kind::Text,
            x,
            y,
            w,
            h,
            color,
            text: text.to_string(),
            size,
            elide,
            fx,
            r: a,
            ring,
            drop,
            band,
            // ZoneCaps.css banner band: inset(0.3em, 0, 0.4em) of the cap.
            band_y: above + 0.3 * cap,
            band_h: 0.3 * cap,
            ..Prim::default()
        });
    }
}

/// The page at `w` by `h` logical px.
pub fn scene(m: &Measure, w: f32, h: f32) -> Scene {
    let p = plan(w, h);
    let a = p.a;
    let mut b = Builder {
        m,
        a,
        prims: Vec::with_capacity(160),
    };
    let level = top_level(p.w_art);
    let least = level != TopLevel::Inline;
    let words = level != TopLevel::Keys;
    let c1 = 10.0 * a; // caps-1
    let c2 = 20.0 * a; // caps-2
                       // The shell's margin and gap (ZoneShell.css).
    let margin = if least {
        4.0 * a
    } else {
        (0.016 * w).clamp(4.0 * a, 11.0 * a)
    };
    let strip_gap = if least { 2.0 * a } else { 5.0 * a };
    // A panel's padding beside its words, beyond its slant (in the strips).
    let strip_pad = if least { 2.0 * a } else { 5.0 * a };

    let top_h = p.tab_h * a;
    let foot_h = p.foot_h * a;
    let body_y = top_h;
    let body_h = (h - top_h - foot_h).max(0.0);

    top_strip(&mut b, w, top_h, c1, c2, margin, strip_gap, level, words);

    // ---- the body: band, facts line, actions ----
    let facts_h = 18.0 * a; // fact-pad-y * 2 + fact-line
    let act_h = p.act_h * a;
    let band_h = (body_h - facts_h - act_h).max(0.0);
    let tris = band(&mut b, &p, w, body_y, band_h);

    // FIND: the body's top right corner (ZoneGames.css).
    {
        let bh = 2.0 * c1;
        let run = bh - a;
        let tw = m.width("FIND", c1);
        let bw = tw + 2.0 * (run + 5.0 * a);
        let x = w - 12.0 * a - bw;
        let y = body_y + 4.0 * a;
        b.panel(x, y, bw, bh, run, BLACK);
        b.words(
            "FIND",
            x + run + 5.0 * a,
            y + (bh - c1) / 2.0,
            c1,
            Ink::White,
            None,
        );
    }

    facts_line(&mut b, w, body_y + band_h, c1);
    actions(&mut b, w, body_y + band_h + facts_h, act_h, c1);
    foot(&mut b, w, h - foot_h, foot_h, c1, margin, strip_pad);

    Scene {
        prims: b.prims,
        tris,
        plan: p,
    }
}

#[allow(clippy::too_many_arguments)]
fn top_strip(
    b: &mut Builder,
    w: f32,
    top_h: f32,
    c1: f32,
    c2: f32,
    margin: f32,
    gap: f32,
    level: TopLevel,
    words: bool,
) {
    let a = b.a;
    let m = b.m;
    let cy = top_h / 2.0;
    let least = level != TopLevel::Inline;
    let pad_y = if least { 1.0 * a } else { 5.0 * a };

    // The tabs: GAMES' banner, then OPTIONS and SAVE DATA (ZoneTab.css).
    let banner_tw = m.width("GAMES", c2);
    let banner_h = 2.0 * c1;
    let point = 0.45 * banner_h;
    let banner_w = banner_tw + 2.0 * (point + 6.0 * a);
    let tab_h = c1 + 2.0 * pad_y;
    let tab_run = tab_h - a;
    let labels = ["OPTIONS", "SAVE DATA"];
    let tab_ws: Vec<f32> = labels
        .iter()
        .map(|l| m.width(l, c1) + 2.0 * (tab_run + 2.0 * a))
        .collect();
    let total = banner_w + tab_ws.iter().sum::<f32>() + gap * labels.len() as f32;
    let mut x = (w - total) / 2.0;

    // The banner: a blue hexagon, the caps over it, the red and green lip.
    let by = cy - banner_h / 2.0;
    let mid = banner_h / 2.0;
    b.poly(
        &[
            (x + point, by),
            (x, by + mid - a / 2.0),
            (x, by + mid + a / 2.0),
            (x + point, by + banner_h),
            (x + banner_w - point, by + banner_h),
            (x + banner_w, by + mid + a / 2.0),
            (x + banner_w, by + mid - a / 2.0),
            (x + banner_w - point, by),
        ],
        BANNER_BLUE,
    );
    b.words(
        "GAMES",
        x + point + 6.0 * a,
        by - 0.5 * a,
        c2,
        Ink::Banner,
        None,
    );
    // The lip hangs over the caps' drop: it follows the body in Zone's DOM.
    // The lip (ZoneBanner.css, Modern): 2 art px, notched 45 degrees at its centre.
    {
        let lx0 = x + point;
        let lx1 = x + banner_w - point;
        let ly = by + banner_h;
        let half = (lx0 + lx1) / 2.0;
        let t = 2.0 * a;
        let d = 4.0 * a;
        b.poly(
            &[
                (lx0, ly),
                (half, ly),
                (half, ly + t),
                (half - 2.0 * a, ly + d),
                (half - 4.0 * a, ly + t),
                (half - 6.0 * a, ly + d),
                (half - 8.0 * a, ly + t),
                (lx0, ly + t),
            ],
            RED,
        );
        b.poly(
            &[
                (half, ly),
                (lx1, ly),
                (lx1, ly + t),
                (half + 8.0 * a, ly + t),
                (half + 6.0 * a, ly + d),
                (half + 4.0 * a, ly + t),
                (half + 2.0 * a, ly + d),
                (half, ly + t),
            ],
            GREEN,
        );
    }
    x += banner_w + gap;
    for (label, tw) in labels.iter().zip(&tab_ws) {
        let ty = cy - tab_h / 2.0;
        b.panel(x, ty, *tw, tab_h, tab_run, BLACK);
        b.words(
            label,
            x + tab_run + 2.0 * a,
            ty + pad_y,
            c1,
            Ink::White,
            None,
        );
        x += tw + gap;
    }

    // The hints: B in the left corner, A in the right (ZoneKeycap.css).
    let d = 1.5 * c1;
    let hint_pad = if least { 2.0 * a } else { 5.0 * a };
    let hint_w = |word: &str| -> f32 {
        if words {
            d / 2.0 + d / 2.0 + hint_pad + m.width(word, c1) + 11.0 * a + 3.0 * a
        } else {
            d
        }
    };
    let hints = [
        ("B", "BACK", KEY_B, margin),
        ("A", "CONFIRM", KEY_A, w - margin - hint_w("CONFIRM")),
    ];
    for (key, word, ink, hx) in hints {
        let ky = cy - d / 2.0;
        if words {
            // The word's black tail, behind the keycap.
            let wx = hx + d / 2.0;
            let ww = hint_w(word) - d / 2.0;
            let wy = cy - 6.0 * a;
            let wh = 12.0 * a;
            b.poly(
                &[
                    (wx, wy),
                    (wx, wy + wh),
                    (wx + ww - 11.0 * a, wy + wh),
                    (wx + ww - 11.0 * a, wy + 11.0 * a),
                    (wx + ww, wy),
                ],
                BLACK,
            );
            b.words_in(
                word,
                wx + d / 2.0 + hint_pad,
                wy + a,
                c1,
                Ink::White,
                (wx, wy, ww, wh),
            );
        }
        b.disc(hx, ky, d, BLACK);
        b.disc(hx + a, ky + a, d - 2.0 * a, KEY_DISC);
        let kw = m.width(key, c1);
        b.words(
            key,
            hx + (d - kw) / 2.0,
            ky + 2.0 * a,
            c1,
            Ink::Flat(ink),
            None,
        );
    }
}

/// The card band: the visible cards, the focused one centred with its
/// cursor. Returns the two triangles.
fn band(b: &mut Builder, p: &Plan, w: f32, band_y: f32, band_h: f32) -> Vec<Tri> {
    let a = b.a;
    let n = p.visible as f32;
    let gap = p.gap * a;
    let tri = p.tri * a;
    let tri_room = tri + 8.0 * a;
    // ZoneCardBand.css: the width the cards do not get, then the fit.
    let fixed = (n - 1.0) * gap + 2.0 * tri_room + 2.0 * 12.0 * a;
    let fit = (w - fixed) / (0.78 * (1.0 + (n - 1.0) * 0.68));
    let card_h = (133.0 * a).min(band_h - 26.0 * a).min(fit).max(0.0);
    let focus_w = card_h * 0.78;
    let side_h = card_h * 0.68;
    let side_w = side_h * 0.78;
    let centre = FOCUS as f32 * (side_w + gap) + tri_room + focus_w / 2.0;
    // Modern: the travel rounds to a layout unit (1/64 px), not an art px.
    let track_x = ((w / 2.0 - centre) * 64.0).round() / 64.0;
    let cy = band_y + band_h / 2.0;
    let reach = (p.visible - 1) / 2;

    let mut x = track_x;
    let mut tris = Vec::new();
    for (i, card) in CARDS.iter().enumerate() {
        let focused = i == FOCUS;
        let (cw, ch) = if focused {
            (focus_w, card_h)
        } else {
            (side_w, side_h)
        };
        if focused {
            x += tri_room;
        }
        let shown = i + reach >= FOCUS && i <= FOCUS + reach;
        if shown && ch > 0.0 {
            draw_card(b, card, focused, x, cy - ch / 2.0, cw, ch);
        }
        if focused {
            let th = tri * 1.1;
            tris.push(Tri {
                edge: x,
                cy,
                w: tri,
                h: th,
                near: 2.0 * a,
                far: 4.0 * a,
                side: -1,
            });
            tris.push(Tri {
                edge: x + cw,
                cy,
                w: tri,
                h: th,
                near: 2.0 * a,
                far: 4.0 * a,
                side: 1,
            });
            x += tri_room;
        }
        x += cw + gap;
    }
    tris
}

/// Greedy lines of a title at cap `cap` inside `room` px, or None when one
/// word alone is wider (ZoneCard.css: no title then).
fn title_lines(m: &Measure, title: &str, cap: f32, room: f32) -> Option<Vec<String>> {
    let space = m.width(" ", cap);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut line_w = 0.0;
    for word in title.split_whitespace() {
        let ww = m.width(word, cap);
        if ww > room + 0.5 {
            return None;
        }
        if line.is_empty() {
            line.push_str(word);
            line_w = ww;
        } else if line_w + space + ww <= room + 0.5 {
            line.push(' ');
            line.push_str(word);
            line_w += space + ww;
        } else {
            lines.push(std::mem::take(&mut line));
            line.push_str(word);
            line_w = ww;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    Some(lines)
}

fn draw_card(b: &mut Builder, card: &Card, focused: bool, x: f32, y: f32, w: f32, h: f32) {
    let a = b.a;
    let m = b.m;
    let c1 = 10.0 * a;
    // Drop, focus edge, body (ZoneCard.css).
    if focused {
        b.rect(x + 3.0 * a, y + 3.0 * a, w, h, SHADE);
        b.rect(x - 2.0 * a, y - 2.0 * a, w + 4.0 * a, h + 4.0 * a, PINK);
    } else {
        b.rect(x + 2.0 * a, y + 2.0 * a, w, h, SHADE);
    }
    b.rect(x, y, w, h, BLACK);
    let frame = if focused { 4.0 * a } else { 2.0 * a };
    let stripe = if focused { 6.0 * a } else { 4.0 * a };
    let ix = x + frame;
    let iw = w - 2.0 * frame;

    // The title: caps-2 on the focused card, stepping to caps-1; caps-1 on
    // a side card; one line while it fits, then wrapped between words.
    let mut cap = if focused { 2.0 * c1 } else { c1 };
    let mut lines = None;
    while lines.is_none() && cap >= c1 {
        let one = m.width(card.title, cap);
        if one <= iw + 0.5 {
            lines = Some(vec![card.title.to_string()]);
        } else if cap <= c1 {
            lines = title_lines(m, card.title, cap, iw);
            break;
        } else {
            cap -= c1;
        }
    }
    let lines = lines.unwrap_or_default();
    let n = lines.len() as f32;
    let title_h = if n > 0.0 {
        n * 1.4 * cap - 0.4 * cap
    } else {
        0.0
    };
    let tag_h = 1.55 * c1;
    let body_h = 2.0 * a + title_h + 2.0 * a + stripe + 2.0 * a + tag_h;
    let body_y = y + h - frame - body_h;

    // The cover well: 1 art px blue edge, black inside, the art filling its
    // width from the top (taller art is cut at the bottom; wider art is
    // centred, letterboxed).
    let well_y = y + frame;
    let well_h = (body_y - well_y).max(0.0);
    if well_h > 0.0 {
        b.rect(ix, well_y, iw, well_h, BLUE);
        let (cx, cy, cw, ch) = (
            ix + a,
            well_y + a,
            iw - 2.0 * a,
            (well_h - 2.0 * a).max(0.0),
        );
        b.rect(cx, cy, cw, ch, BLACK);
        let art_h = cw / card.art_ratio;
        let iy = if art_h > ch { 0.0 } else { (ch - art_h) / 2.0 };
        b.prims.push(Prim {
            kind: Kind::Cover,
            x: cx,
            y: cy,
            w: cw,
            h: ch,
            art: card.art,
            ix: 0.0,
            iy,
            iw: cw,
            ih: art_h,
            ..Prim::default()
        });
    }

    for (i, line) in lines.iter().enumerate() {
        let lw = m.width(line, cap);
        let cap_top = body_y + 2.0 * a + i as f32 * 1.4 * cap;
        b.words(line, ix + (iw - lw) / 2.0, cap_top, cap, Ink::White, None);
    }

    let stripe_y = body_y + 2.0 * a + title_h + 2.0 * a;
    stripe_bands(b, ix, stripe_y, iw, stripe);

    tag_row(b, card.tags, ix, stripe_y + stripe + 2.0 * a, iw, tag_h, c1);
}

/// TIME ATTACK's stripe (ZoneCard.css): `linear-gradient(-62deg, light 0
/// 56%, blue 56% 80%, teal 80% 100%)` on hard stops, drawn as three
/// polygons cut from the box along CSS's gradient line.
fn stripe_bands(b: &mut Builder, x: f32, y: f32, w: f32, h: f32) {
    let angle = (-62.0f32).to_radians();
    let (dx, dy) = (angle.sin(), -angle.cos());
    let length = (w * angle.sin()).abs() + (h * angle.cos()).abs();
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    // The share of the gradient line at a point.
    let at = move |(px, py): (f32, f32)| ((px - cx) * dx + (py - cy) * dy) / length + 0.5;
    let rect = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    // Each band runs to the far end and the next paints over it, so no
    // seam of the card's black shows between two antialiased edges.
    b.rect(x, y, w, h, STRIPE_LIGHT);
    for (from, colour) in [(0.56, BLUE), (0.80, STRIPE_TEAL)] {
        let band = clip(&rect, |p| at(p) - from);
        if band.len() >= 3 {
            b.poly(&band, colour);
        }
    }
}

/// The part of a convex polygon where `side` is not negative.
fn clip(poly: &[(f32, f32)], side: impl Fn((f32, f32)) -> f32) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let p = poly[i];
        let q = poly[(i + 1) % poly.len()];
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
        }
    }
    out
}

/// The tag row (ZoneTag.css, ZoneTagRow.tsx): tags joined on their slants,
/// sharing the row; the last ones drop when the row cannot hold them.
fn tag_row(b: &mut Builder, tags: &[(TagKind, &str)], x: f32, y: f32, w: f32, h: f32, cap: f32) {
    let a = b.a;
    let m = b.m;
    let slant = (6.0 * a).min(0.55 * cap);
    let slant_pad = slant / 2.0 + a;
    // The joins come from the whole set: a tag the row hides keeps its
    // neighbour's cut (ZoneTagRow.tsx hides tags, it does not re-join them).
    let count = tags.len();
    let pads = |i: usize| -> (f32, f32) {
        let lp = if i == 0 { a } else { slant_pad };
        let rp = if i + 1 == count { a } else { slant_pad };
        (lp, rp)
    };
    let row_w = |shown: usize| -> f32 {
        tags.iter()
            .take(shown)
            .enumerate()
            .map(|(i, (_, label))| {
                let (lp, rp) = pads(i);
                lp + m.width(label, cap) + rp
            })
            .sum::<f32>()
            - slant * shown.saturating_sub(1) as f32
    };
    // Chromium shrinks the tags to the row in proportion to their size and
    // calls a tag cut when its scroll width passes its client width by more
    // than 1 px (ZoneTagRow.tsx `cut`); both are whole px there, so a tag
    // may lose about 1.5 px before it counts as cut.
    let cut = |shown: usize| -> bool {
        let over = row_w(shown) - w;
        if over <= 0.0 {
            return false;
        }
        let bases: Vec<f32> = tags
            .iter()
            .take(shown)
            .enumerate()
            .map(|(i, (_, label))| {
                let (lp, rp) = pads(i);
                lp + m.width(label, cap) + rp
            })
            .collect();
        let total: f32 = bases.iter().sum();
        bases.iter().any(|b| over * b / total > 1.5)
    };
    let mut shown = count;
    while shown > 0 && cut(shown) {
        shown -= 1;
    }
    if shown == 0 {
        return;
    }
    let extra = (w - row_w(shown)) / shown as f32;
    let mut tx = x;
    for (i, (kind, label)) in tags.iter().take(shown).enumerate() {
        let (lp, rp) = pads(i);
        let tw = m.width(label, cap);
        let tag_w = lp + tw + rp + extra;
        let first = i == 0;
        let last = i + 1 == count;
        let (bg, ink) = match kind {
            TagKind::System => (BLUE, Ink::Tag),
            TagKind::Here => (YELLOW, Ink::Flat(BLACK)),
            TagKind::Remote => (RED, Ink::Tag),
            TagKind::Continue => (BLACK, Ink::Tag),
        };
        let l_top = if first { 0.0 } else { slant };
        let r_bottom = if last { 0.0 } else { slant };
        b.poly(
            &[
                (tx + l_top, y),
                (tx + tag_w, y),
                (tx + tag_w - r_bottom, y + h),
                (tx, y + h),
            ],
            bg,
        );
        let content = tag_w - lp - rp;
        b.words_in(
            label,
            tx + lp + (content - tw) / 2.0,
            y + (h - cap) / 2.0,
            cap,
            ink,
            (tx, y, tag_w, h),
        );
        tx += tag_w - slant;
    }
}

/// Korri's session facts for Hollow Knight on one black 45-degree strip.
fn facts_line(b: &mut Builder, w: f32, y: f32, cap: f32) {
    let a = b.a;
    let m = b.m;
    let facts = [
        ("LAST PLAYED", "10 Oct 2026"),
        ("PLAYS", "7"),
        ("TIME", "3h 12m"),
    ];
    let pair = 3.0 * a;
    let between = 7.0 * a;
    let lean = 14.0 * a;
    let run_w: f32 = facts
        .iter()
        .map(|(l, v)| m.width(l, cap) + pair + m.width(v, cap))
        .sum::<f32>()
        + between * (facts.len() - 1) as f32;
    let panel_w = run_w + 2.0 * (lean + 2.0 * a);
    let panel_h = 18.0 * a;
    let px = (w - panel_w) / 2.0;
    b.panel(px, y, panel_w, panel_h, lean, BLACK);
    let cap_top = y + 4.0 * a;
    let mut x = px + lean + 2.0 * a;
    for (label, value) in facts {
        b.words(label, x, cap_top, cap, Ink::Hud, None);
        x += m.width(label, cap) + pair;
        b.words(value, x, cap_top, cap, Ink::White, None);
        x += m.width(value, cap) + between;
    }
}

/// PLAY (lit: its one place is the default) and OPTS.
fn actions(b: &mut Builder, w: f32, y: f32, h: f32, cap: f32) {
    let a = b.a;
    let m = b.m;
    let bh = 2.0 * cap;
    let run = bh - a;
    let gap = 1.4 * cap + 8.0 * a + 5.0 * a;
    let labels = ["PLAY", "OPTS."];
    let ws: Vec<f32> = labels
        .iter()
        .map(|l| m.width(l, cap) + 2.0 * (run + 5.0 * a))
        .collect();
    let total = ws.iter().sum::<f32>() + gap;
    let mut x = (w - total) / 2.0;
    let by = y + (h - bh) / 2.0;
    for (i, (label, bw)) in labels.iter().zip(&ws).enumerate() {
        if i == 0 {
            // The pink underline: the panel's shape, 2 art px down and left.
            b.panel(x - 2.0 * a, by + 2.0 * a, *bw, bh, run, PINK);
        }
        b.panel(x, by, *bw, bh, run, BLACK);
        b.words(
            label,
            x + run + 5.0 * a,
            by + (bh - cap) / 2.0,
            cap,
            Ink::White,
            None,
        );
        x += bw + gap;
    }
}

/// The foot: TIME and the clock, the Wi-Fi badge, the build (ZoneFoot.css).
fn foot(b: &mut Builder, w: f32, y: f32, h: f32, cap: f32, margin: f32, pad: f32) {
    let a = b.a;
    let m = b.m;
    let gap = (0.016 * w).clamp(4.0 * a, 5.0 * a);
    let ph = h - 8.0 * a;
    let py = y + 4.0 * a;
    let run = ph - a;
    let side = run + pad;
    let cap_top = py + (ph - cap) / 2.0;

    let time_w = m.width("TIME", cap);
    let clock_w = m.width("21:47", cap);
    let timer_w = 2.0 * side + time_w + 3.0 * a + clock_w;

    let ssid = "hummingbird-lake";
    let strength = "72%";
    let bars_w = 4.0 * 2.0 * a + 3.0 * a;
    let fixed_signal = 2.0 * side + 5.0 * a + bars_w + 5.0 * a + m.width(strength, cap);
    let signal_natural = fixed_signal + m.width(ssid, cap);
    let build = "korri 0.4.1 \u{b7} 9a1fb22";
    let build_natural = m.width(build, cap);

    // Flex: the spacer takes what is left; with nothing left, the badge and
    // the build shrink in proportion to their size, and their words elide.
    let room = w - 2.0 * margin - 3.0 * gap - timer_w;
    let both = signal_natural + build_natural;
    let (signal_w, build_w) = if both <= room {
        (signal_natural, build_natural)
    } else {
        let over = both - room;
        (
            signal_natural - over * signal_natural / both,
            build_natural - over * build_natural / both,
        )
    };

    let tx = margin;
    b.panel(tx, py, timer_w, ph, run, BLACK);
    b.words("TIME", tx + side, cap_top, cap, Ink::Hud, None);
    b.words(
        "21:47",
        tx + side + time_w + 3.0 * a,
        cap_top,
        cap,
        Ink::White,
        None,
    );

    let bx = w - margin - build_w;
    let sx = bx - gap - signal_w;
    b.panel(sx, py, signal_w, ph, run, BLACK);
    let name_w = (signal_w - fixed_signal).max(0.0);
    b.words(ssid, sx + side, cap_top, cap, Ink::White, Some(name_w));
    let mut x = sx + side + name_w + 5.0 * a;
    let bars_y = py + (ph - 8.0 * a) / 2.0;
    for i in 0..4 {
        let bh = 2.0 * a * (i + 1) as f32;
        let lit = i < 3;
        b.rect(
            x,
            bars_y + 8.0 * a - bh,
            2.0 * a,
            bh,
            if lit { WHITE } else { TRACK },
        );
        x += 3.0 * a;
    }
    x += -a + 5.0 * a;
    b.words(strength, x, cap_top, cap, Ink::White, None);

    b.words(build, bx, cap_top, cap, Ink::Flat(BLACK), Some(build_w));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_pixel_matches_zone() {
        assert_eq!(art_px(640.0, 480.0), 2.0);
        assert_eq!(art_px(1280.0, 720.0), 3.0);
        assert_eq!(art_px(1920.0, 1080.0), 4.0);
        assert_eq!(art_px(1280.0, 300.0), 1.0);
    }

    #[test]
    fn plan_answers_match_zone() {
        let small = plan(640.0, 480.0);
        assert_eq!((small.visible, small.tab_h, small.tri), (3, 36.0, 12.0));
        let big = plan(1280.0, 720.0);
        assert_eq!((big.visible, big.tab_h, big.tri), (5, 36.0, 14.0));
        let tall = plan(480.0, 800.0);
        assert_eq!((tall.visible, tall.tab_h), (3, 40.0));
        assert_eq!(top_level(320.0), TopLevel::Keys);
        assert_eq!(top_level(426.0), TopLevel::Tight);
    }
}
