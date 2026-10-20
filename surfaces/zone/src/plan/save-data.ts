/**
 * The SAVE DATA page's layout decisions and text shapes, as pure functions.
 *
 * Height decides structure, width decides content (as in zone-select.ts):
 *
 *  - The backup's white slab stands under the band while the page is a
 *    column, and beside it once the page is wider than tall by
 *    `ZONE_SAVE_SLAB_BESIDE_ASPECT`. Height is what the QR code in the
 *    BACK UP card needs to be scannable (the card's size follows the band's
 *    height), so wherever width can pay for the slab, it does. A 640x480 or
 *    1280x300 screen keeps the band's full height; a 480x800 column stacks.
 *    The page's stylesheet holds the same number in its container query;
 *    test/plan-save-data.test.ts holds the two equal.
 *  - The SAVE DATA banner folds away once the page is a strip wider than
 *    tall by `ZONE_SAVE_BANNER_FOLD_ASPECT` (1280x300 is 1280 by 198 of
 *    body). The tab strip above already names the page, so the banner is the
 *    first thing to go, and the band gets its height back.
 *  - The secret is drawn in groups of `ZONE_SECRET_GROUP` characters,
 *    `ZONE_SECRET_LINE` to a line, so a person copying it by hand can keep
 *    their place. A line too wide for its slab wraps whole groups, never
 *    inside one.
 *  - A public key is shortened to its first and last `ZONE_KEY_END`
 *    characters, which is what a person compares between two screens.
 */

/** Body width over body height at and above which the slab stands beside the band. */
export const ZONE_SAVE_SLAB_BESIDE_ASPECT = 1.25
/** Body width over body height at and above which the banner folds away. */
export const ZONE_SAVE_BANNER_FOLD_ASPECT = 4
/** Characters in one group of the secret. */
export const ZONE_SECRET_GROUP = 4
/** Characters on one line of the secret. */
export const ZONE_SECRET_LINE = 32
/** Characters kept from each end of a shortened key. */
export const ZONE_KEY_END = 8

export type ZoneSaveSlab = "under" | "beside"

export interface ZoneSaveDataPlan {
  readonly slab: ZoneSaveSlab
  /** Whether the SAVE DATA banner is drawn; the tab strip names the page either way. */
  readonly banner: boolean
}

/** The page's plan for a body of this size. */
export function planSaveData(width: number, height: number): ZoneSaveDataPlan {
  const aspect = height > 0 ? width / height : 0
  const slab: ZoneSaveSlab = aspect >= ZONE_SAVE_SLAB_BESIDE_ASPECT ? "beside" : "under"
  return { slab, banner: aspect < ZONE_SAVE_BANNER_FOLD_ASPECT }
}

/** The secret as lines of groups: `ZONE_SECRET_LINE` characters a line, `ZONE_SECRET_GROUP` a group. */
export function secretLines(secret: string): readonly (readonly string[])[] {
  const lines: string[][] = []
  for (let start = 0; start < secret.length; start += ZONE_SECRET_LINE) {
    const line = secret.slice(start, start + ZONE_SECRET_LINE)
    const groups: string[] = []
    for (let at = 0; at < line.length; at += ZONE_SECRET_GROUP) groups.push(line.slice(at, at + ZONE_SECRET_GROUP))
    lines.push(groups)
  }
  return lines
}

/** A key's first and last `ZONE_KEY_END` characters; a key that short is shown whole. */
export function shortKey(key: string): string {
  return key.length <= ZONE_KEY_END * 2 + 1
    ? key
    : `${key.slice(0, ZONE_KEY_END)}…${key.slice(-ZONE_KEY_END)}`
}
