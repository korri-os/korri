/**
 * The CRT in the viewing room, and everything Korri says about a launch.
 *
 * There is no status panel: the TV says it, the way a VCR prints on the
 * screen. Standby glass when nothing is in; a blue VCR screen while Korri
 * works; the loaded game's cover once it runs; a red screen with Korri's own
 * words when it fails, and which button on the deck does what.
 */
import { useEffect, useMemo } from "react"
import * as THREE from "three"
import type { TvStatus } from "./boxbuster-store-view"
import { CONSOLE_Z, type StoreGame, TV_SCREEN } from "./map"
import { createPS1Material } from "./ps1-material"
import { loadCoverImage, wrap } from "./textures"

const W = 256
const H = 192 // 4:3 CRT
const PAD = 14

export function TvScreen({
  playing,
  tv,
}: {
  playing: StoreGame | null
  tv: TvStatus
}) {
  const gear = useMemo(() => {
    const canvas = document.createElement("canvas")
    canvas.width = W
    canvas.height = H
    const ctx = canvas.getContext("2d")
    if (!ctx) throw new Error("boxbuster: 2d canvas context unavailable")
    const texture = new THREE.CanvasTexture(canvas)
    texture.magFilter = THREE.NearestFilter
    texture.minFilter = THREE.NearestFilter
    texture.generateMipmaps = false
    texture.colorSpace = THREE.SRGBColorSpace
    const mat = createPS1Material({ map: texture, emissive: true })
    return { ctx, texture, mat }
  }, [])

  // The status is rebuilt on every model; redraw only when what it says changes.
  const said = JSON.stringify(tv)

  useEffect(() => {
    const { ctx, texture } = gear
    const done = () => {
      scanlines(ctx)
      texture.needsUpdate = true
    }
    if (tv._tag === "Problem") {
      screen(ctx, "#3a0b10", "#12040a")
      let y = PAD + 18
      y = say(ctx, tv.kicker.toUpperCase(), y, 20, "#ff7a72", 2)
      if (tv.title !== undefined) y = say(ctx, tv.title, y + 4, 14, "#f2c100", 1)
      y = say(ctx, tv.reason, y + 4, 14, "#f4e9c1", 3)
      const keys = [
        ...(tv.canRetry ? ["PLAY TO TRY AGAIN"] : []),
        "EJECT TO TAKE IT OUT",
      ]
      keys.forEach((line, i) =>
        say(ctx, line, H - PAD - (keys.length - 1 - i) * 16, 12, "#9fb6e0", 1),
      )
      done()
      return
    }
    if (tv._tag === "Working") {
      // the blue screen a VCR shows while it works
      screen(ctx, "#1d3fa8", "#0f2470")
      let y = PAD + 22
      y = say(ctx, tv.kicker.toUpperCase(), y, 22, "#ffffff", 2)
      if (tv.detail !== undefined) {
        y = say(ctx, tv.detail, y + 6, 14, "#cfe0ff", tv.lines === undefined ? 3 : 2)
      }
      // A choice lists each piece of work in Korri's words, one line each,
      // in the same order as the signed decks below the TV.
      for (const line of tv.lines ?? []) {
        if (y + 4 > H - PAD) break
        y = say(ctx, line, y + 4, 12, "#f2c100", 1)
      }
      done()
      return
    }
    if (playing === null) {
      standby(ctx)
      done()
      return
    }
    // a tape is in: its cover fills the screen once it loads
    screen(ctx, "#000000", "#000000")
    const caption =
      tv._tag === "Playing" ? `▶ ${tv.kicker.toUpperCase()}` : "▶ PLAY"
    say(ctx, playing.title, PAD + 18, 18, "#f4e9c1", 3)
    banner(ctx, caption)
    done()
    let alive = true
    if (playing.coverArtUrl !== undefined) {
      loadCoverImage(playing.coverArtUrl).then(img => {
        if (!alive || img === null) return
        ctx.imageSmoothingEnabled = false
        const { sx, sy, sw, sh } = crop(img, W / H)
        ctx.drawImage(img, sx, sy, sw, sh, 0, 0, W, H)
        banner(ctx, caption)
        done()
      })
    }
    return () => {
      alive = false
    }
  }, [playing, said, gear])

  return (
    <mesh
      position={[0, TV_SCREEN.y, TV_SCREEN.z - CONSOLE_Z]}
      material={gear.mat}
    >
      <planeGeometry args={[TV_SCREEN.w, TV_SCREEN.h]} />
    </mesh>
  )
}

function screen(ctx: CanvasRenderingContext2D, top: string, bottom: string) {
  const grad = ctx.createLinearGradient(0, 0, 0, H)
  grad.addColorStop(0, top)
  grad.addColorStop(1, bottom)
  ctx.fillStyle = grad
  ctx.fillRect(0, 0, W, H)
}

/** Print wrapped text from `y` (its first baseline); returns the next free y. */
function say(
  ctx: CanvasRenderingContext2D,
  text: string,
  y: number,
  size: number,
  color: string,
  maxLines: number,
): number {
  ctx.font = `bold ${size}px monospace`
  ctx.textBaseline = "alphabetic"
  ctx.textAlign = "left"
  ctx.fillStyle = color
  const lines = wrap(ctx, text, W - PAD * 2)
  const shown = lines.slice(0, maxLines)
  if (lines.length > maxLines && shown.length > 0) {
    shown[shown.length - 1] = `${shown[shown.length - 1]}…`
  }
  shown.forEach((line, i) => {
    ctx.fillStyle = "#000000"
    ctx.fillText(line, PAD + 2, y + i * (size + 3) + 2)
    ctx.fillStyle = color
    ctx.fillText(line, PAD, y + i * (size + 3))
  })
  return y + shown.length * (size + 3)
}

function banner(ctx: CanvasRenderingContext2D, caption: string) {
  ctx.fillStyle = "rgba(0,0,0,0.7)"
  ctx.fillRect(0, H - 30, W, 30)
  say(ctx, caption, H - 10, 16, "#f2c100", 1)
}

function scanlines(ctx: CanvasRenderingContext2D) {
  ctx.fillStyle = "rgba(0,0,0,0.18)"
  for (let y = 0; y < H; y += 2) ctx.fillRect(0, y, W, 1)
}

/** Dark glassy CRT in standby: gradient, a reflection sheen, a red LED. */
function standby(ctx: CanvasRenderingContext2D) {
  screen(ctx, "#0b121c", "#04060a")
  ctx.save()
  ctx.globalAlpha = 0.1
  ctx.fillStyle = "#9fc8ff"
  ctx.beginPath()
  ctx.moveTo(0, H * 0.18)
  ctx.lineTo(W * 0.55, 0)
  ctx.lineTo(W * 0.85, 0)
  ctx.lineTo(0, H * 0.62)
  ctx.closePath()
  ctx.fill()
  ctx.restore()
  ctx.fillStyle = "#a01b1b"
  ctx.fillRect(W - 14, H - 12, 6, 6)
}

/** Cover-crop an image to the screen's aspect, centred. */
function crop(img: HTMLImageElement, ratio: number) {
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
