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
