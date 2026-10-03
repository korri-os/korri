import * as THREE from "three"
import type { StoreGame } from "./map"

// All textures are generated on a <canvas> so the surface ships zero assets.
// The only images it loads are the cover-art URLs Korri publishes.
// Everything is point-filtered + mip-less to stay crunchy (very PS1).

function tex(canvas: HTMLCanvasElement): THREE.CanvasTexture {
  const t = new THREE.CanvasTexture(canvas)
  t.magFilter = THREE.NearestFilter
  t.minFilter = THREE.NearestFilter
  t.generateMipmaps = false
  t.wrapS = THREE.RepeatWrapping
  t.wrapT = THREE.RepeatWrapping
  t.colorSpace = THREE.SRGBColorSpace
  return t
}

function mk(
  w: number,
  h: number,
): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const c = document.createElement("canvas")
  c.width = w
  c.height = h
  const ctx = c.getContext("2d")
  if (!ctx) throw new Error("boxbuster: 2d canvas context unavailable")
  ctx.imageSmoothingEnabled = false
  return [c, ctx]
}

// deterministic tiny PRNG so reloads look the same
function rng(seed: number) {
  let s = seed >>> 0
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0
    return s / 0xffffffff
  }
}

export function carpetTexture(): THREE.CanvasTexture {
  const [c, ctx] = mk(64, 64)
  ctx.fillStyle = "#10243a"
  ctx.fillRect(0, 0, 64, 64)
  const r = rng(7)
  // teal/blue speckle with the odd warm fleck — generic 90s rental carpet
  for (let i = 0; i < 900; i++) {
    const x = (r() * 64) | 0
    const y = (r() * 64) | 0
    const warm = r() > 0.9
    ctx.fillStyle = warm
      ? `rgb(${180 + r() * 60},${80 + r() * 40},${40})`
      : `rgb(${20 + r() * 30},${70 + r() * 60},${110 + r() * 50})`
    ctx.fillRect(x, y, 1, 1)
  }
  return tex(c)
}

export function wallTexture(): THREE.CanvasTexture {
  const [c, ctx] = mk(64, 64)
  const g = ctx.createLinearGradient(0, 0, 0, 64)
  g.addColorStop(0, "#15356e")
  g.addColorStop(1, "#0c1f47")
  ctx.fillStyle = g
  ctx.fillRect(0, 0, 64, 64)
  // faint horizontal trim line
  ctx.fillStyle = "#f2c100"
  ctx.fillRect(0, 44, 64, 2)
  ctx.fillStyle = "#0a1730"
  ctx.fillRect(0, 46, 64, 1)
  return tex(c)
}

export function ceilingTexture(): THREE.CanvasTexture {
  const [c, ctx] = mk(32, 32)
  ctx.fillStyle = "#141823"
  ctx.fillRect(0, 0, 32, 32)
  ctx.strokeStyle = "#1d2230"
  ctx.strokeRect(0.5, 0.5, 31, 31)
  return tex(c)
}

// n-th entry of a non-empty list, wrapping around.
function cycle<T>(list: readonly [T, ...T[]], n: number): T {
  return list[n % list.length] ?? list[0]
}

const GENRE_BANDS: readonly [
  readonly [string, string],
  ...(readonly [string, string])[],
] = [
  ["#c81d25", "#f2c100"],
  ["#1d3fc8", "#27c8b0"],
  ["#7a1dc8", "#e84bd0"],
  ["#0a8a2a", "#bfe800"],
  ["#c8521d", "#f2a200"],
  ["#1d1d1d", "#9a9a9a"],
]

const FAKE_TITLES: readonly [string, ...string[]] = [
  "NIGHT RUN",
  "STARFALL",
  "COLD STEEL",
  "THE HEIST",
  "DINO PARK",
  "ZONE 9",
  "RUSH HOUR",
  "DEAD DRIFT",
  "OVERBOARD",
  "LASER COP",
  "BIG SPLASH",
  "ROBO WAR",
  "GHOST MALL",
  "RED OCTANE",
  "SKY PILOT",
  "TURBO KIDS",
]

// The de-facto portrait cover grid is 600x900 = 2:3. Match the tape faces and
// atlas cells to it so most covers fit with no cropping.
export const COVER_RATIO = 2 / 3 // width / height
const FRONT_CW = 64
const FRONT_CH = Math.round(FRONT_CW / COVER_RATIO) // 96

// cover-crop a source image to a target aspect (centre) and return src rect
function coverCrop(img: HTMLImageElement, ratio: number) {
  const sar = img.width / img.height
  let sw = img.width
  let sh = img.height
  let sx = 0
  let sy = 0
  if (sar > ratio) {
    sw = sh * ratio
    sx = (img.width - sw) / 2
  } else {
    sh = sw / ratio
    sy = (img.height - sh) / 2
  }
  return { sx, sy, sw, sh }
}

// A single atlas of VHS covers; meshes pick a cell via UV offset/repeat. Cell
// n belongs to games[n]: a procedural cover with the real title first, then
// the published cover art once it loads.
export function vhsAtlas(
  cols: number,
  rows: number,
  games: readonly StoreGame[],
): THREE.CanvasTexture {
  const cw = FRONT_CW
  const ch = FRONT_CH
  const [c, ctx] = mk(cols * cw, rows * ch)
  const r = rng(42)
  let n = 0
  for (let ry = 0; ry < rows; ry++) {
    for (let rx = 0; rx < cols; rx++) {
      const ox = rx * cw
      const oy = ry * ch
      const [a, b] = cycle(GENRE_BANDS, n)
      // body
      ctx.fillStyle = "#0d0d10"
      ctx.fillRect(ox, oy, cw, ch)
      // cover art block
      const g = ctx.createLinearGradient(ox, oy, ox, oy + ch)
      g.addColorStop(0, a)
      g.addColorStop(1, b)
      ctx.fillStyle = g
      ctx.fillRect(ox + 3, oy + 3, cw - 6, ch - 18)
      // a crude "scene" silhouette
      ctx.fillStyle = "rgba(0,0,0,0.45)"
      for (let i = 0; i < 4; i++) {
        const sx = ox + 4 + r() * (cw - 12)
        const sh = 6 + r() * 30
        ctx.fillRect(sx, oy + ch - 18 - sh, 3 + r() * 6, sh)
      }
      // title bar
      ctx.fillStyle = "#08080a"
      ctx.fillRect(ox + 3, oy + ch - 14, cw - 6, 11)
      ctx.fillStyle = "#f4e9c1"
      ctx.font = "bold 7px monospace"
      ctx.textBaseline = "middle"
      const title = games[n]?.title.toUpperCase() ?? cycle(FAKE_TITLES, n)
      ctx.fillText(title.slice(0, 11), ox + 5, oy + ch - 8)
      // spine highlight
      ctx.fillStyle = "rgba(255,255,255,0.08)"
      ctx.fillRect(ox + 3, oy + 3, 2, ch - 6)
      n++
    }
  }
  const t = tex(c)
  t.wrapS = THREE.ClampToEdgeWrapping
  t.wrapT = THREE.ClampToEdgeWrapping
  // Progressively replace the procedural placeholders with real, downsampled
  // cover art (pico's technique: point-sample to the tiny cell, no smoothing —
  // but no palette remap, so PS1's fuller colour survives).
  loadRealCovers(ctx, cw, ch, cols, games.slice(0, cols * rows), t)
  return t
}

// Cover art is drawn into a WebGL texture, and a canvas tainted by a
// cross-origin image cannot be uploaded. Requesting CORS means an image the
// server will not share fails to load and the placeholder stays, instead.
export function loadCoverImage(url: string): Promise<HTMLImageElement | null> {
  return new Promise(resolve => {
    const image = new Image()
    image.crossOrigin = "anonymous"
    image.onload = () => resolve(image)
    image.onerror = () => resolve(null)
    image.src = url
  })
}

// pico-style downsample: cover-crop the source to the cell's 2:3 ratio and draw
// it into the tiny cell with smoothing off (nearest-neighbour). No quantize.
function loadRealCovers(
  ctx: CanvasRenderingContext2D,
  cw: number,
  ch: number,
  cols: number,
  games: readonly StoreGame[],
  t: THREE.CanvasTexture,
) {
  const drawInto = (img: HTMLImageElement, n: number) => {
    const ox = (n % cols) * cw
    const oy = Math.floor(n / cols) * ch
    const { sx, sy, sw, sh } = coverCrop(img, cw / ch)
    ctx.imageSmoothingEnabled = false
    ctx.clearRect(ox, oy, cw, ch)
    ctx.drawImage(img, sx, sy, sw, sh, ox, oy, cw, ch)
    t.needsUpdate = true
  }

  // bounded-concurrency pool so 64 covers don't all request at once
  let next = 0
  const worker = async () => {
    while (next < games.length) {
      const n = next++
      const url = games[n]?.coverArtUrl
      if (url === undefined) continue
      const img = await loadCoverImage(url)
      if (img) drawInto(img, n)
    }
  }
  for (let w = 0; w < 5; w++) worker()
}

// Word-wrap helper for canvas text.
function wrap(
  ctx: CanvasRenderingContext2D,
  text: string,
  maxW: number,
): string[] {
  const words = text.split(" ")
  const lines: string[] = []
  let line = ""
  for (const w of words) {
    const test = line ? `${line} ${w}` : w
    if (ctx.measureText(test).width > maxW && line) {
      lines.push(line)
      line = w
    } else {
      line = test
    }
  }
  if (line) lines.push(line)
  return lines
}

// Per-game back-cover atlas — same cell grid as the front, so a tape's back
// shows ITS game's title laid out like a real box back. Higher-res cells
// (128px) keep the big title legible in-world. Korri publishes no blurb,
// genre, year, or screenshots, so those areas stay printed placeholders.
export function gameBackAtlas(
  cols: number,
  rows: number,
  allGames: readonly StoreGame[],
): THREE.CanvasTexture {
  const cw = 128
  const ch = Math.round(cw / COVER_RATIO) // 192 — match the 2:3 box back
  const [c, ctx] = mk(cols * cw, rows * ch)
  const games = allGames.slice(0, cols * rows)
  for (const [i, g] of games.entries()) {
    const ox = (i % cols) * cw
    const oy = Math.floor(i / cols) * ch

    ctx.fillStyle = "#0a0a12"
    ctx.fillRect(ox, oy, cw, ch)
    // header
    ctx.fillStyle = "#c81d25"
    ctx.fillRect(ox + 4, oy + 4, cw - 8, 14)
    ctx.fillStyle = "#f4e9c1"
    ctx.font = "bold 8px monospace"
    ctx.textBaseline = "middle"
    ctx.textAlign = "left"
    ctx.fillText((g.subtitle ?? "").toUpperCase().slice(0, 18), ox + 7, oy + 11)
    // title (wrapped, big)
    ctx.fillStyle = "#ffffff"
    ctx.font = "bold 11px monospace"
    const titleLines = wrap(ctx, g.title, cw - 14).slice(0, 3)
    titleLines.forEach((ln, k) => {
      ctx.fillText(ln, ox + 7, oy + 28 + k * 12)
    })
    let y = oy + 28 + titleLines.length * 12 + 6
    // screenshot slots — 3 across, 16:9-ish, printed as colour blocks
    const shotW = Math.floor((cw - 14 - 8) / 3)
    const shotH = Math.round((shotW * 9) / 16)
    for (let s = 0; s < 3; s++) {
      ctx.fillStyle = cycle(GENRE_BANDS, i + s)[s % 2 === 0 ? 0 : 1]
      ctx.fillRect(ox + 7 + s * (shotW + 4), y, shotW, shotH)
    }
    y += shotH + 8
    // synopsis lines, printed as grey rules
    const lines = rng(i + 7)
    ctx.fillStyle = "#5a6072"
    for (let k = 0; k < 6; k++)
      ctx.fillRect(ox + 7, y + k * 9, 30 + lines() * (cw - 44), 2)
    // barcode
    ctx.fillStyle = "#fff"
    ctx.fillRect(ox + cw - 50, oy + ch - 18, 44, 13)
    ctx.fillStyle = "#000"
    const br = rng(i + 1)
    for (let x = ox + cw - 48; x < ox + cw - 8; x += 2)
      ctx.fillRect(x, oy + ch - 16, br() > 0.5 ? 1 : 1.6, 9)
  }
  const t = tex(c)
  t.wrapS = THREE.ClampToEdgeWrapping
  t.wrapT = THREE.ClampToEdgeWrapping
  return t
}

export function bannerTexture(
  text: string,
  bg = "#0c1f47",
  fg = "#f2c100",
): THREE.CanvasTexture {
  const [c, ctx] = mk(256, 64)
  ctx.fillStyle = bg
  ctx.fillRect(0, 0, 256, 64)
  ctx.fillStyle = fg
  ctx.fillRect(0, 6, 256, 4)
  ctx.fillRect(0, 54, 256, 4)
  ctx.font = "bold 34px Arial, sans-serif"
  ctx.textBaseline = "middle"
  ctx.textAlign = "center"
  ctx.fillStyle = fg
  ctx.fillText(text, 128, 34)
  const t = tex(c)
  t.wrapS = THREE.ClampToEdgeWrapping
  t.wrapT = THREE.ClampToEdgeWrapping
  return t
}

export function posterTexture(seed: number): THREE.CanvasTexture {
  const [c, ctx] = mk(64, 96)
  const r = rng(seed * 131 + 17)
  const [a, b] = cycle(GENRE_BANDS, seed)
  const g = ctx.createLinearGradient(0, 0, 0, 96)
  g.addColorStop(0, a)
  g.addColorStop(1, "#05060c")
  ctx.fillStyle = g
  ctx.fillRect(0, 0, 64, 96)
  ctx.fillStyle = b
  ctx.beginPath()
  ctx.arc(32, 38, 10 + r() * 8, 0, Math.PI * 2)
  ctx.fill()
  ctx.fillStyle = "rgba(0,0,0,0.5)"
  for (let i = 0; i < 6; i++) {
    const sx = r() * 64
    const sh = 10 + r() * 30
    ctx.fillRect(sx, 96 - sh, 4 + r() * 8, sh)
  }
  ctx.fillStyle = "#000"
  ctx.fillRect(4, 78, 56, 14)
  ctx.fillStyle = "#f4e9c1"
  ctx.font = "bold 10px monospace"
  ctx.textBaseline = "middle"
  ctx.textAlign = "center"
  ctx.fillText(cycle(FAKE_TITLES, seed).slice(0, 9), 32, 85)
  const t = tex(c)
  t.wrapS = THREE.ClampToEdgeWrapping
  t.wrapT = THREE.ClampToEdgeWrapping
  return t
}
