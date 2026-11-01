/**
 * Zone's three looks: one axis of rendering fidelity, from modern to retro.
 * Zone's identity (layout, palette, grounds, banner, cards, dialogs, title
 * card, tally, HUD, attract, focus) is the same in every look; only how
 * finely it is drawn changes.
 *
 *     axis                   modern              blend (default)     retro
 *     Zone's own words       Anton caps, smooth  Zone Caps (pixel)   Zone Caps (pixel)
 *                            outline and drop
 *     Korri's facts          Anton, own case     Anton, own case     Jersey 10 (pixel)
 *     pixel grid             sizes on --zone-px, whole art px        whole art px
 *                            drawing smooth
 *     edges                  smooth 45 degrees   stairs              stairs
 *     dimming under dialogs  a see-through veil  dither              dither
 *     rings                  smooth, antialiased pixel               pixel
 *     motion                 eased, sub-pixel    whole art px        whole art px
 *
 * Retro is the pixel refinement exactly as it was before the looks. The
 * root wears the look as `data-look` (zone-tokens.css switches the faces
 * and tokens); what TypeScript draws (stairs, the ground, motion) asks
 * `zoneLookDrawing` how to draw for the look its element stands in.
 */

export const ZONE_LOOKS = ["modern", "blend", "retro"] as const
export type ZoneLook = (typeof ZONE_LOOKS)[number]

/** The look a device starts in, and the look of anything unreadable. */
export const ZONE_DEFAULT_LOOK: ZoneLook = "blend"

export function isZoneLook(value: unknown): value is ZoneLook {
  return (ZONE_LOOKS as readonly unknown[]).includes(value)
}

/** The look's name on the LOOK row: Zone's own word, in caps. */
export const ZONE_LOOK_WORDS: Readonly<Record<ZoneLook, string>> = {
  modern: "MODERN",
  blend: "BLEND",
  retro: "RETRO",
}

/** Left (-1) or right (1) along the axis, round the ends, as every choice steps. */
export function zoneLookStep(look: ZoneLook, direction: -1 | 1): ZoneLook {
  const at = ZONE_LOOKS.indexOf(look)
  return ZONE_LOOKS[(at + direction + ZONE_LOOKS.length) % ZONE_LOOKS.length]!
}

/** A 45 degree edge: a stair of one step per art px, or one smooth line. */
export type ZoneEdges = "stairs" | "smooth"

/** A ring on the ground: pixels on the art grid, or an antialiased arc. */
export type ZoneRings = "pixel" | "smooth"

/** Where a moving thing may stand: on whole art px, or anywhere. */
export type ZoneMotionGrid = "art" | "free"

export interface ZoneLookDrawing {
  readonly edges: ZoneEdges
  readonly rings: ZoneRings
  readonly motion: ZoneMotionGrid
}

/** How TypeScript draws for a look; Blend and Retro differ only in their facts' face. */
export function zoneLookDrawing(look: ZoneLook): ZoneLookDrawing {
  return look === "modern"
    ? { edges: "smooth", rings: "smooth", motion: "free" }
    : { edges: "stairs", rings: "pixel", motion: "art" }
}
