/**
 * A size in bytes as Zone writes it: the one formatter for the download tag,
 * the GAMES facts line and the busy strip's meter. Korri never ships a
 * label for a size (the treaty's `SurfaceGame.download`), so Zone owns it.
 *
 *     under 1 KB          whole bytes          512 B
 *     under 10 MB         whole KB or MB       640 KB, 9 MB
 *     10 MB and over      one decimal          12.5 MB, 1.2 GB, 1.0 GB
 *
 * Units are decimal (1 KB = 1000 B), as the portal's own detail lines count
 * them, so "1.5 GB" on a card and in Korri's question are the same number.
 * GB is the largest unit. A value that rounds up to 1000 of its unit is
 * written in the next unit instead (999 999 B is 1 MB, not 1000 KB). A
 * negative or non-finite count is written as 0 B.
 */

const KB = 1_000
const MB = 1_000_000
const GB = 1_000_000_000

/** From this size up, one decimal; below it, whole numbers. */
export const ZONE_BYTES_DECIMAL_FROM = 10 * MB

/** Largest first. */
const UNITS = [
  { size: GB, name: "GB" },
  { size: MB, name: "MB" },
  { size: KB, name: "KB" },
] as const

const rounded = (value: number, decimals: number): number => {
  const scale = 10 ** decimals
  return Math.round(value * scale) / scale
}

export function formatZoneBytes(bytes: number): string {
  const count = Number.isFinite(bytes) && bytes > 0 ? bytes : 0
  if (count < KB) return `${Math.round(count)} B`
  const decimals = count >= ZONE_BYTES_DECIMAL_FROM ? 1 : 0
  let index = UNITS.findIndex(unit => count >= unit.size)
  let value = rounded(count / UNITS[index]!.size, decimals)
  // 999 999 B rounds to 1000 KB: it is written as 1 MB.
  if (value >= 1000 && index > 0) {
    index -= 1
    value = rounded(count / UNITS[index]!.size, decimals)
  }
  return `${value.toFixed(decimals)} ${UNITS[index]!.name}`
}
