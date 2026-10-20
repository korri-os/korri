/**
 * The idle title card's one layout decision. The act circle stands beside
 * the title while the card is at least as wide as tall, and under it once
 * the card is taller than wide: a narrow-tall card then gives the title its
 * whole width instead of a column beside the circle, which once stacked a
 * title one letter to a line at 480x800.
 *
 * ZoneTitleCard.css holds the same number in its container query;
 * test/zone-title-card.test.tsx holds the two equal.
 */

/** Card width over height under which the circle moves below the title. */
export const ZONE_TITLE_STACK_ASPECT = 1

export type ZoneTitleCircle = "beside" | "under"

export function planTitleCard(width: number, height: number): ZoneTitleCircle {
  return height > 0 && width / height < ZONE_TITLE_STACK_ASPECT ? "under" : "beside"
}

/**
 * The title card's four bands (REFINEMENT.md 2c, the title card), as
 * shares of the card: where each band's left edge meets its top row, the
 * row it starts on, and its thickness across. Each falls at 45 degrees to
 * the bottom right and runs off the card. Listed bottom to top, as they
 * are painted: green under orange under red under blue.
 */
export const ZONE_TITLE_BANDS = [
  { colour: "green", x: 0.56, y: 0.46, thickness: 0.07 },
  { colour: "orange", x: 0.1, y: 0.3, thickness: 0.2 },
  { colour: "red", x: 0.12, y: 0.58, thickness: 0.4 },
  { colour: "blue", x: 0.15, y: 0, thickness: 0.16 },
] as const

/** The act disc's diameter in art px. */
export const ZONE_TITLE_DISC = 52

/**
 * One band as a stair polygon on a card `wArt` by `hArt`: from its start
 * row to the card's foot, its two edges moving one art px right a row, so
 * the 45 degree edges are stairs with one step per art px.
 */
export function zoneTitleBandClip(band: { readonly x: number; readonly y: number; readonly thickness: number }, wArt: number, hArt: number): string {
  const px = (value: number) => (value === 0 ? "0" : `calc(${value} * var(--zone-px))`)
  const top = Math.round(band.y * hArt)
  const left = Math.round(band.x * wArt)
  const thick = Math.max(1, Math.round(band.thickness * wArt))
  const rows = Math.max(0, hArt - top)
  if (rows === 0 || wArt <= 0) return "polygon(0 0, 0 0, 0 0)"
  const leftEdge: string[] = []
  const rightEdge: string[] = []
  for (let row = 0; row < rows; row += 1) {
    const y = top + row
    leftEdge.push(`${px(left + row)} ${px(y)}`, `${px(left + row)} ${px(y + 1)}`)
    rightEdge.push(`${px(left + thick + row)} ${px(y + 1)}`, `${px(left + thick + row)} ${px(y)}`)
  }
  return `polygon(${[...leftEdge, ...rightEdge.reverse()].join(", ")})`
}
