/**
 * The results tally's words and numbers (REFINEMENT.md 3, M5): the rows
 * Korri's facts allow, each value as the tally writes it, and where a
 * count-up stands at a share of its run. Pure; the tally organism draws
 * them and its clock picks the share.
 *
 *     TIME   this session, minutes and seconds: 72'14"
 *     PLAYS  the new count
 *     TOTAL  the new total, hours and minutes: 3H 12M (12M under an hour)
 */

export interface ZoneTallyFacts {
  readonly seconds?: number
  readonly plays: number
  readonly total?: number
}

export type ZoneTallyRowKind = "time" | "plays" | "total"

export interface ZoneTallyRow {
  readonly kind: ZoneTallyRowKind
  readonly label: string
  /** The bar's length at the end of the count, 0 to 1. */
  readonly share: number
}

/** The rows the facts allow, in order: TIME only when the session's length is known. */
export function zoneTallyRows(facts: ZoneTallyFacts): readonly ZoneTallyRow[] {
  const whole = facts.total !== undefined && facts.total > 0 && facts.seconds !== undefined
    ? Math.min(1, facts.seconds / facts.total) : 1
  return [
    ...(facts.seconds === undefined ? [] : [{ kind: "time", label: "Time", share: whole } as const]),
    { kind: "plays", label: "Plays", share: 1 } as const,
    ...(facts.total === undefined ? [] : [{ kind: "total", label: "Total", share: 1 } as const]),
  ]
}

/** Minutes and seconds: 72'14". */
export function zoneTallyTime(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds))
  return `${Math.floor(whole / 60)}'${String(whole % 60).padStart(2, "0")}"`
}

/** Hours and minutes: 3H 12M, or 12M under an hour. */
export function zoneTallyTotal(seconds: number): string {
  const minutes = Math.floor(Math.max(0, seconds) / 60)
  const hours = Math.floor(minutes / 60)
  return hours > 0 ? `${hours}H ${minutes % 60}M` : `${minutes % 60}M`
}

/** A row's value as written at the end of its count. */
export function zoneTallyValue(kind: ZoneTallyRowKind, facts: ZoneTallyFacts): string {
  switch (kind) {
    case "time":
      return zoneTallyTime(facts.seconds ?? 0)
    case "plays":
      return String(facts.plays)
    case "total":
      return zoneTallyTotal(facts.total ?? 0)
  }
}

/**
 * The numbers a row shows at `count` (0 to 1) of its run, as the custom
 * properties its stylesheet counts with: TIME's minutes and seconds, the
 * PLAYS count, TOTAL's hours and minutes. Whole numbers only.
 */
export function zoneTallyCounts(kind: ZoneTallyRowKind, facts: ZoneTallyFacts, count: number): Readonly<Record<string, number>> {
  const share = Math.max(0, Math.min(1, count))
  switch (kind) {
    case "time": {
      const seconds = Math.floor((facts.seconds ?? 0) * share)
      return { "--zone-tally-time-m": Math.floor(seconds / 60), "--zone-tally-time-s": seconds % 60 }
    }
    case "plays":
      return { "--zone-tally-plays": Math.floor(facts.plays * share) }
    case "total": {
      const minutes = Math.floor(((facts.total ?? 0) * share) / 60)
      return { "--zone-tally-total-h": Math.floor(minutes / 60), "--zone-tally-total-m": minutes % 60 }
    }
  }
}
