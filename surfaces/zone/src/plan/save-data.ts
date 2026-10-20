/**
 * The SAVE DATA page's layout decisions and text shapes, as pure functions.
 *
 * Height decides structure, width decides content (as in zone-select.ts):
 *
 *  - Korri's backup (BackupReady) takes the band's room: the QR code leaves
 *    the BACK UP card's well and becomes the largest square the room
 *    allows, so a phone can read it, with the secret's white slab beside
 *    it, or under it once the page is taller than wide
 *    (`ZONE_SAVE_BACKUP_BESIDE_ASPECT`). The slab keeps at least
 *    `--zone-save-slab-least` across (beside) or `--zone-save-slab-least-block`
 *    down (under); the square gets everything else. The page's stylesheet
 *    holds the same aspect in its container query;
 *    test/plan-save-data.test.ts holds the two equal.
 *  - A retired key's BACK UP / DELETE row stands at the band's end, not
 *    under it, once the page is a strip wider than tall by
 *    `ZONE_SAVE_ROW_BESIDE_ASPECT` (1280x300 is 1280 by 198 of body): such
 *    a strip has no height to give a row under the band.
 *  - The secret is drawn in groups of `ZONE_SECRET_GROUP` characters,
 *    `ZONE_SECRET_LINE` to a line, so a person copying it by hand can keep
 *    their place. A line too wide for its slab wraps whole groups, never
 *    inside one.
 *  - A public key is shortened to its first and last `ZONE_KEY_END`
 *    characters, which is what a person compares between two screens.
 */

/** Body width over body height at and above which the backup's slab stands beside its QR code. */
export const ZONE_SAVE_BACKUP_BESIDE_ASPECT = 1
/** Body width over body height at and above which a retired key's row stands beside the band. */
export const ZONE_SAVE_ROW_BESIDE_ASPECT = 4
/** Characters in one group of the secret. */
export const ZONE_SECRET_GROUP = 4
/** Characters on one line of the secret. */
export const ZONE_SECRET_LINE = 32
/** Characters kept from each end of a shortened key. */
export const ZONE_KEY_END = 8

export type ZoneSaveSlab = "under" | "beside"

export interface ZoneSaveDataPlan {
  /** Where the backup's slab stands against its QR code. */
  readonly slab: ZoneSaveSlab
  /** Where a retired key's row stands against the band. */
  readonly row: "under" | "beside"
}

/** The page's plan for a body of this size. */
export function planSaveData(width: number, height: number): ZoneSaveDataPlan {
  const aspect = height > 0 ? width / height : 0
  const slab: ZoneSaveSlab = aspect >= ZONE_SAVE_BACKUP_BESIDE_ASPECT ? "beside" : "under"
  return { slab, row: aspect >= ZONE_SAVE_ROW_BESIDE_ASPECT ? "beside" : "under" }
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
