/**
 * Shift's words for save sync (ADR 0014, CONTEXT.md "Saves").
 *
 * Korri sends raw facts: a UTC epoch-millisecond end time by the device's own
 * clock, a session length in seconds, and a tagged save state per game. Shift
 * owns every word the person reads. End times are display only: Shift never
 * orders, compares or decides anything by them.
 *
 * Vocabulary: "split" and "save history", never "conflict".
 */
import type { SurfaceGame } from "@contracts/surface/korri-surface"

export type ShiftSaveState = NonNullable<SurfaceGame["saves"]>["_tag"]

/** The mark a game carries for its saves. Wording is Shift's. */
export const SAVE_MARK_LABELS: Readonly<Record<ShiftSaveState, string>> = {
  Synced: "Saves synced",
  Split: "Saves split",
  NotChecked: "Not checked with other devices",
  NotSynced: "Saves not synced",
  SyncedBySteam: "Synced by Steam",
}

/** The split notice, as the contract states it. */
export const SPLIT_NOTICE = "Saves split between two devices"

export function saveMarkLabel(
  saves: SurfaceGame["saves"],
): string | undefined {
  return saves === undefined ? undefined : SAVE_MARK_LABELS[saves._tag]
}

/** Whether a mark asks the person for something (only a split does). */
export function saveMarkNeedsChoice(saves: SurfaceGame["saves"]): boolean {
  return saves?._tag === "Split"
}

const DAY_MS = 86_400_000

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()
}

/**
 * "Today at 4:12 PM", "Yesterday at 9:03 PM", "Monday at 9:03 PM",
 * "Oct 12 at 9:03 PM", "Oct 12, 2025". Local time. A clock ahead of this
 * device (a future end) is stated as its date, never as "in 3 hours".
 */
export function formatSaveEndedAt(
  endedAt: number,
  now: Date = new Date(),
  locale?: string,
): string {
  if (!Number.isFinite(endedAt)) return "Unknown time"
  const ended = new Date(endedAt)
  const time = new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
  }).format(ended)
  const days = Math.round((startOfDay(now) - startOfDay(ended)) / DAY_MS)
  if (days === 0) return `Today at ${time}`
  if (days === 1) return `Yesterday at ${time}`
  if (days > 1 && days < 7) {
    const weekday = new Intl.DateTimeFormat(locale, { weekday: "long" }).format(ended)
    return `${weekday} at ${time}`
  }
  if (ended.getFullYear() === now.getFullYear()) {
    const day = new Intl.DateTimeFormat(locale, { month: "short", day: "numeric" }).format(ended)
    return `${day} at ${time}`
  }
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "short",
    day: "numeric",
  }).format(ended)
}

/** The end time inside a sentence: "today at 4:12 PM", "on Oct 12 at 9:03 PM". */
export function formatSaveEndedAtInSentence(
  endedAt: number,
  now: Date = new Date(),
  locale?: string,
): string {
  const label = formatSaveEndedAt(endedAt, now, locale)
  if (label.startsWith("Today ") || label.startsWith("Yesterday ")) {
    return label.charAt(0).toLowerCase() + label.slice(1)
  }
  return label === "Unknown time" ? "at an unknown time" : `on ${label}`
}

/** "Under a minute", "45 min", "1 h", "1 h 12 min", "1,204 h 5 min". */
export function formatSaveDuration(durationSeconds: number, locale?: string): string {
  const seconds = Number.isFinite(durationSeconds) ? Math.max(0, durationSeconds) : 0
  const minutes = Math.floor(seconds / 60)
  if (minutes < 1) return "Under a minute"
  if (minutes < 60) return `${minutes} min`
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  const hoursLabel = `${hours.toLocaleString(locale)} h`
  return rest === 0 ? hoursLabel : `${hoursLabel} ${rest} min`
}

/** The session-length line on a split card and a history row. */
export function formatSavePlayed(durationSeconds: number, locale?: string): string {
  const duration = formatSaveDuration(durationSeconds, locale)
  return `Played ${duration === "Under a minute" ? "under a minute" : duration}`
}
