// PROTOTYPE — throwaway. See index.html for the question and the Plymouth facts.
//
// One timeline, as a Plymouth `script` theme would run it (50 fps refresh):
//   1. sprout   t=0        the leaf grows once from the stem, overshoots, settles, then sways
//   2. signal   t=S        `plymouth display-message --text=korri:wordmark`, sent by the
//                          compositor unit as it starts; the leaf glides down to become
//                          the i-dot while "korri" rises (700 ms)
//   3. quit     t=Q        `plymouth quit --retain-splash`: the theme's quit function
//                          snaps to the finished wordmark and the frame is held until the
//                          portal paints the same wordmark
// A script theme cannot animate after quit (script/plugin.c: become_idle pulls the
// trigger at once), which is why the signal exists and why quit snaps.
// Two clocks, both simulated:
//   render  — the theme's refresh: Plymouth.SetRefreshRate(n) in the script (default 50)
//   scanout — the panel: RG353M 60 Hz (st7703 rg353v2_mode), Odin 120 Hz (icna3512 mode 0)
// Each scanout shows the newest rendered frame. 50 into 60 repeats one frame in six.
// The render tick re-arms after the frame's work (script/plugin.c on_timeout), so on
// the device the true cadence is 1/rate + draw time; that is not simulated.
const SRC = { cx: 160, cy: 249.5, h: 347.6, px: 33, py: 423 }; // leaf, source units
const LEAF_FRAC = 1 / 3; // leaf height as a fraction of panel height (step 1)
const WORD_FRAC = 0.6; // wordmark width as a fraction of panel width (step 2)
const DEVICES = {
  rg353m: { w: 640, h: 480, zoom: 2, hz: 60 },
  odin: { w: 1920, h: 1080, zoom: 0.6, hz: 120 },
};

// Wordmark geometry, mirrored from brand/build-leaf-mark.py so the prototype
// can vary the gap and the leaf size without touching brand/. If a value here
// wins, it moves into build-leaf-mark.py.
const WORD = { petiole: [33.0, 423.0], iTop: [1250.5, 197.6], ascender: 315 };
const url = new URL(location);
const num = (k, d) => (url.searchParams.has(k) ? Number(url.searchParams.get(k)) : d);
// The type's own i-dot sits 27 units above the stem (measured from leaf-source.svg
// path 10 vs path 9). brand/ ships the sprout at 10; the dot's gap is the eye's reference.
const GAP = num('gap', 27); // units between petiole and i stem (brand: 10, type's dot: 27)
const IDOT = num('idot', 0.34); // leaf height as a fraction of the ascender (brand: 0.34)
const ARRIVAL = url.searchParams.get('arrival') === 'glide' ? 'glide' : 'fall';
let FPS = [50, 60, 120].includes(num('fps', 0)) ? num('fps', 0) : null; // null = device default

// Timing constants — every number that could be argued about lives here.
const T = {
  grow: 900, // sprout duration
  growRot: -30, // degrees the sprout leans at t=0, about the petiole
  swayFrom: 700, // sway starts fading in
  swayFade: 600, // ...over this long
  swayAmp: 1.5, // degrees
  swayPeriod: 2600, // ms
  // arrival=glide (previous version)
  glide: 700, // signal → wordmark settled
  lettersFrom: 250, // letters start rising this long after the signal
  // arrival=fall
  windup: 160, // the last sway swing exaggerates before the drop
  windupAmp: 6, // degrees
  fall: 240, // ease-in drop from centre to the i
  squash: 90, // contact squash, then recover
  squashAmt: 0.12, // 12% flatter on contact
  glyphStep: 40, // ms per glyph in the snap-in, i → k (160 ms total; 20 read as flicker)
};

const $ = (s) => document.querySelector(s);
const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
const lerp = (a, b, t) => a + (b - a) * t;
const easeInOut = (t) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);
const easeOut = (t) => 1 - (1 - t) ** 3;
const easeOutBack = (t) => 1 + 2.70158 * (t - 1) ** 3 + 1.70158 * (t - 1) ** 2;

const SVGNS = 'http://www.w3.org/2000/svg';
const el = (tag, attrs = {}, children = []) => {
  const e = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  e.append(...children);
  return e;
};
const clone = (nodes) => nodes.map((n) => document.importNode(n, true));
const about = (x, y, inner) => `translate(${x} ${y}) ${inner} translate(${-x} ${-y})`;
const place = (g, { tx, ty, s }, extra = '') =>
  g.setAttribute('transform', `translate(${tx} ${ty}) scale(${s}) ${extra}`);

async function loadArt() {
  const get = async (u) =>
    new DOMParser().parseFromString(await (await fetch(u)).text(), 'image/svg+xml').documentElement;
  const mark = await get('/brand/korri-mark.svg');
  const word = await get('/brand/korri-wordmark-dark.svg');
  const [tx, ty, s] = word.querySelector('g').getAttribute('transform').match(/-?[\d.]+/g).map(Number);
  return {
    leaf: [...mark.querySelector('g').children], // [silhouette, ...two-tone leaf]
    letters: [...word.children].filter((n) => n.tagName === 'path'),
    word: { tx, ty, s, vb: word.getAttribute('viewBox').split(/\s+/).map(Number) },
  };
}

// Where the leaf sits in each step, in panel pixels.
function layout(dev, art) {
  const s1 = (dev.h * LEAF_FRAC) / SRC.h;
  const centred = { tx: dev.w / 2 - SRC.cx * s1, ty: dev.h / 2 - SRC.cy * s1, s: s1 };
  // Leaf placement on the wordmark, recomputed from WORD with the prototype's GAP/IDOT.
  const ws = (WORD.ascender * IDOT) / SRC.h;
  const wtx = WORD.iTop[0] - WORD.petiole[0] * ws;
  const wty = WORD.iTop[1] - GAP - WORD.petiole[1] * ws;
  const top = wty + 75.6 * ws - 20;
  const vb = [400, top, art.word.vb[0] + art.word.vb[2] - 400, 425 - top];
  const ks = (WORD_FRAC * dev.w) / vb[2];
  const ox = dev.w / 2 - (vb[0] + vb[2] / 2) * ks;
  const oy = dev.h / 2 - (vb[1] + vb[3] / 2) * ks;
  const idot = { tx: ox + wtx * ks, ty: oy + wty * ks, s: ws * ks };
  return { centred, idot, letters: { ox, oy, ks } };
}

function createScene(svg, dev, art) {
  const L = layout(dev, art);
  // Glyph order for the snap-in: nearest the i first. Letter paths in the
  // wordmark are left→right (k, o, r, r, i-stem), so reverse.
  const glyphs = clone(art.letters).map((p) => el('g', { opacity: 0 }, [p])).reverse();
  const letters = el('g', {}, glyphs);
  const leaf = el('g', {}, clone(art.leaf));
  svg.append(letters, leaf);

  // The whole theme is this one pure function of (t, signalAt, quit).
  return ({ t, signalAt, quit }) => {
    const grow = clamp(t / T.grow);
    const swayIn = clamp((t - T.swayFrom) / T.swayFade);
    const s = signalAt == null ? -1 : t - signalAt; // ms since the signal
    let k = 0; // 0 = centred, 1 = seated on the i
    let extraRot = 0;
    let squash = 1;
    let glyphOn = 0; // how many glyphs are visible (fall) or fade level (glide)
    let info;

    if (ARRIVAL === 'glide') {
      k = s < 0 ? 0 : easeInOut(clamp(s / T.glide));
      glyphOn = s < 0 ? 0 : easeOut(clamp((s - T.lettersFrom) / (T.glide - T.lettersFrom)));
      info = k < 1 ? `glide ${Math.round(k * 100)}%` : 'settled, waiting for quit';
    } else if (s >= 0) {
      if (s < T.windup) {
        // wind-up: exaggerate the swing in the direction of the fall
        extraRot = T.windupAmp * Math.sin((Math.PI * s) / T.windup);
        info = 'wind-up';
      } else if (s < T.windup + T.fall) {
        const f = (s - T.windup) / T.fall;
        k = f * f; // ease-in: gravity
        extraRot = T.windupAmp * (1 - f) * -0.5; // lean into the fall, then level
        info = `fall ${Math.round(k * 100)}%`;
      } else {
        k = 1;
        const q = (s - T.windup - T.fall) / T.squash;
        squash = q < 1 ? 1 - T.squashAmt * Math.sin(Math.PI * q) : 1;
        const landed = s - T.windup - T.fall;
        glyphOn = Math.min(glyphs.length, 1 + Math.floor(landed / T.glyphStep));
        info = glyphOn < glyphs.length ? `landed, glyph ${glyphOn}/${glyphs.length}` : 'wordmark, waiting for quit';
      }
    }
    if (quit) {
      k = 1;
      squash = 1;
      extraRot = 0;
      glyphOn = ARRIVAL === 'glide' ? 1 : glyphs.length;
      info = 'quit → snapped to wordmark, frame retained';
    }

    const sway = T.swayAmp * swayIn * (1 - k) * Math.sin((2 * Math.PI * t) / T.swayPeriod);
    const rot = (1 - grow) * T.growRot + sway + extraRot;
    const scale = Math.max(easeOutBack(grow), 0.001);
    const at = { tx: lerp(L.centred.tx, L.idot.tx, k), ty: lerp(L.centred.ty, L.idot.ty, k), s: lerp(L.centred.s, L.idot.s, k) };
    // squash: flatten y and widen x about the petiole, volume roughly kept
    place(leaf, at, about(SRC.px, SRC.py, `rotate(${rot}) scale(${scale / squash} ${scale * squash})`));

    if (ARRIVAL === 'glide') {
      letters.setAttribute('opacity', glyphOn);
      for (const g of glyphs) g.setAttribute('opacity', 1);
      letters.setAttribute('transform', `translate(${L.letters.ox} ${L.letters.oy + (1 - glyphOn) * 8}) scale(${L.letters.ks})`);
    } else {
      letters.setAttribute('opacity', 1);
      glyphs.forEach((g, i) => g.setAttribute('opacity', i < glyphOn ? 1 : 0));
      letters.setAttribute('transform', `translate(${L.letters.ox} ${L.letters.oy}) scale(${L.letters.ks})`);
    }

    if (info) return info;
    return grow < 1 ? `grow ${Math.round(grow * 100)}%` : `sway ${sway.toFixed(1)}°`;
  };
}

// --- driver -----------------------------------------------------------------

const panel = $('#panel');
let devKey = DEVICES[url.searchParams.get('device')] ? url.searchParams.get('device') : 'rg353m';
// ?shot=<ms>[&signal=<ms>][&quit] freezes the simulation at a moment, for screenshots.
const shot = url.searchParams.has('shot') ? Number(url.searchParams.get('shot')) : null;

let art, frame, simT, signalAt, quit, tPrev;

function restart() {
  simT = shot ?? 0;
  signalAt = url.searchParams.has('signal') && shot != null ? Number(url.searchParams.get('signal')) : null;
  quit = shot != null && url.searchParams.has('quit');
  tPrev = performance.now();
}

function mount() {
  const dev = DEVICES[devKey];
  if (FPS == null) FPS = dev.hz; // Plymouth.SetRefreshRate(panel Hz): locked, no beat
  url.searchParams.set('device', devKey);
  url.searchParams.set('fps', FPS);
  $('#fps').value = FPS;
  history.replaceState(null, '', url);
  $('#hz').textContent = `panel ${dev.hz} Hz`;
  panel.style.width = `${dev.w * dev.zoom}px`;
  panel.style.height = `${dev.h * dev.zoom}px`;
  panel.replaceChildren(el('svg', { viewBox: `0 0 ${dev.w} ${dev.h}` }));
  frame = createScene(panel.firstChild, dev, art);
  restart();
}

// Scanout bookkeeping: which rendered frame each panel refresh showed.
let lastScan = -1;
let lastShownFrame = -1;
let repeats = [];

function tick(now) {
  if (shot == null && !quit) simT += (now - tPrev) * ($('#slow').checked ? 0.25 : 1);
  tPrev = now;
  const dev = DEVICES[devKey];
  // The panel refresh that is on screen now, and the newest render tick at or before it.
  const scan = Math.floor((simT * dev.hz) / 1000);
  // newest render tick at or before this scanout; integer maths so equal
  // clocks lock exactly instead of drifting on float rounding
  const renderFrame = Math.floor((scan * FPS) / dev.hz);
  const shownT = (renderFrame * 1000) / FPS;
  const info = frame({ t: shownT, signalAt, quit });
  if (scan !== lastScan) {
    if (scan === lastScan + 1 && renderFrame === lastShownFrame) repeats.push(simT);
    lastScan = scan;
    lastShownFrame = renderFrame;
  }
  repeats = repeats.filter((r) => simT - r < 1000);
  const beat = FPS < dev.hz ? `${repeats.length} repeated/s` : FPS > dev.hz ? `${Math.round(FPS - dev.hz)} dropped/s` : 'locked';
  $('#boot').textContent = `t=${(simT / 1000).toFixed(2)} s · render ${renderFrame} @ ${FPS} fps · scanout ${scan} @ ${dev.hz} Hz · ${beat} · ${ARRIVAL} · gap ${GAP} · ${info}`;
  requestAnimationFrame(tick);
}

const signal = () => {
  if (signalAt == null && !quit) signalAt = simT;
};
const doQuit = () => {
  quit = true;
};
$('#signal').onclick = signal;
$('#quit').onclick = doQuit;
$('#restart').onclick = restart;
$('#device').value = devKey;
$('#device').onchange = (e) => {
  devKey = e.target.value;
  FPS = null;
  mount();
};
$('#fps').onchange = (e) => {
  FPS = Number(e.target.value);
  mount();
};
addEventListener('keydown', (e) => {
  if (e.key === 'Enter') signal();
  else if (e.key === 'q') doQuit();
  else if (e.key === 'r') restart();
});

art = await loadArt();
mount();
requestAnimationFrame(tick);
