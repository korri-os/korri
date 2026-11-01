/**
 * The art pixel: the one integer grid every Zone screen stands on.
 *
 * One art pixel is `--zone-px` CSS pixels, a whole number: the short side
 * of the container holds at least ZONE_ART_ROWS of them, as Mania's own
 * canvas is 240 rows tall. 640x480 gives 2 (a 320x240 canvas), 1280x720
 * gives 3 (426x240, Mania's canvas), 1920x1080 gives 4, 480x800 gives 2 and
 * 1280x300 gives 1. Never fractional, never below 1: Zone floors and gives
 * the spare CSS pixels to layout, which keeps every glyph and every stair
 * crisp.
 *
 * Every plan lays itself out in art px: it takes the container's CSS size,
 * divides it here, and states its thresholds and lengths as art px. The
 * root writes the answer as `--zone-px`; zone-tokens.css states every
 * length as a number of art px, and a stylesheet multiplies it by
 * `--zone-px` where it uses it.
 *
 * `--zone-art-rows` in zone-tokens.css is ZONE_ART_ROWS, held equal by
 * test, for parts that stand outside the root: zone.css derives the same
 * pixel there from the container queries.
 */

/** The short side holds at least this many art pixels. */
export const ZONE_ART_ROWS = 240

/** The least art pixel, in CSS px. */
export const ZONE_ART_PX_LEAST = 1

export interface ZoneArt {
  /** CSS px per art px: a whole number, 1 or more. */
  readonly px: number
  /** The container's width in whole art px. */
  readonly wArt: number
  /** The container's height in whole art px. */
  readonly hArt: number
}

/** `max(1, floor(min(width, height) / 240))`. */
export function zoneArtPx(width: number, height: number): number {
  const short = Math.min(width, height)
  return Math.max(ZONE_ART_PX_LEAST, Math.floor(Math.max(0, short) / ZONE_ART_ROWS))
}

/** The container in art px, with the art pixel it was divided by. */
export function zoneArt(width: number, height: number): ZoneArt {
  const px = zoneArtPx(width, height)
  return { px, wArt: Math.floor(Math.max(0, width) / px), hArt: Math.floor(Math.max(0, height) / px) }
}
