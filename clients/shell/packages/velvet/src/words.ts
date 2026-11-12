/**
 * Velvet's own words for the facts Korri publishes as numbers: how long ago,
 * how long played, how many, how big. Pure functions; "now" is the model's
 * `publishedAt` (the moment korrid built it), never a clock of Velvet's own.
 */

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"] as const

/** A day as "6 Nov 2026": what the fact says while Korri gives no time to compare with. */
export function dayOf(at: number): string {
  const date = new Date(at)
  return `${date.getDate()} ${MONTHS[date.getMonth()] ?? ""} ${date.getFullYear()}`
}

/**
 * When a game was last played, against the model's time. `publishedAt` can
 * be a minute behind and a device clock can be wrong, so a session that
 * ended "after" it is "Just now", never a negative time.
 */
export function ago(lastPlayedAt: number | undefined, publishedAt: number | undefined): string {
  if (lastPlayedAt === undefined) return "Not played yet"
  if (publishedAt === undefined) return dayOf(lastPlayedAt)
  const since = publishedAt - lastPlayedAt
  if (since < MINUTE) return "Just now"
  if (since < HOUR) {
    const minutes = Math.floor(since / MINUTE)
    return minutes === 1 ? "A minute ago" : `${minutes} minutes ago`
  }
  if (since < DAY) {
    const hours = Math.round(since / HOUR)
    return hours <= 1 ? "An hour ago" : `${hours} hours ago`
  }
  const days = Math.floor(since / DAY)
  if (days === 1) return "Yesterday"
  if (days < 14) return `${days} days ago`
  if (days < 60) return `${Math.round(days / 7)} weeks ago`
  if (days < 365) return `${Math.round(days / 30.4)} months ago`
  const years = Math.round(days / 365)
  return years <= 1 ? "A year ago" : `${years} years ago`
}

/** The same fact inside a sentence: "Played 2 hours ago", "Played 6 Nov 2026". */
export function played(lastPlayedAt: number | undefined, publishedAt: number | undefined): string {
  if (lastPlayedAt === undefined) return "Not played yet"
  const when = ago(lastPlayedAt, publishedAt)
  return publishedAt === undefined ? `Played ${when}` : `Played ${when.charAt(0).toLowerCase()}${when.slice(1)}`
}

/** Whether a session ended within `days` of the model's time. False without a time to compare with. */
export function within(lastPlayedAt: number | undefined, publishedAt: number | undefined, days: number): boolean {
  if (lastPlayedAt === undefined || publishedAt === undefined) return false
  return publishedAt - lastPlayedAt <= days * DAY
}

/** Time played: "61 h 20 min", "45 min". Undefined when nothing was recorded. */
export function playtime(seconds: number | undefined): string | undefined {
  if (seconds === undefined || seconds <= 0) return undefined
  const minutes = Math.floor(seconds / 60)
  if (minutes < 1) return "Under a minute"
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  return hours > 0 ? (rest > 0 ? `${hours} h ${rest} min` : `${hours} h`) : `${rest} min`
}

/** A session's length in a save's line: "1 h 5 min", "40 s". */
export function duration(seconds: number): string {
  if (seconds < 60) return `${Math.max(0, Math.round(seconds))} s`
  return playtime(seconds) ?? "0 min"
}

export const games = (count: number): string => `${count.toLocaleString("en-US")} ${count === 1 ? "game" : "games"}`

/** A size in bytes: "1.2 GB", "340 MB", "12 KB". */
export function bytes(count: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"] as const
  let value = Math.max(0, count)
  let unit = 0
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit++
  }
  const text = unit === 0 || value >= 100 ? String(Math.round(value)) : value >= 10 ? value.toFixed(1).replace(/\.0$/, "") : value.toFixed(1).replace(/\.0$/, "")
  return `${text} ${units[unit]}`
}

/** A date and time for a save: "6 Nov 2026, 21:47". */
export function moment(at: number): string {
  const date = new Date(at)
  const two = (n: number) => String(n).padStart(2, "0")
  return `${dayOf(at)}, ${two(date.getHours())}:${two(date.getMinutes())}`
}

/**
 * Display caps. Upper-casing turns the micro sign into a Greek capital mu,
 * which reads as M ("µCity" would set as "MCITY"), so the sign keeps its case.
 */
export function caps(text: string): string {
  if (text.indexOf("µ") < 0) return text.toUpperCase()
  return text.split("µ").map(part => part.toUpperCase()).join("µ")
}
