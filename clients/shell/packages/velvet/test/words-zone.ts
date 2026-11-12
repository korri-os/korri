/**
 * Prints Velvet's words about time as JSON, for test/words.test.ts, which
 * runs this file under several `TZ` values and expects the same words from
 * each. The last entry is not Velvet's: it is the local hour of one instant,
 * there to prove the zone under test took effect.
 */
import { ago, dayOf, played, saved, within } from "../src/words"

const NOW = Date.UTC(2026, 10, 11, 21, 47)
const MINUTE = 60_000, HOUR = 60 * MINUTE, DAY = 24 * HOUR
// Instants near midnight in UTC and in zones either side of it.
const instants = [Date.UTC(2026, 10, 6, 0, 30), Date.UTC(2026, 10, 5, 23, 59), Date.UTC(2026, 11, 31, 23, 30), Date.UTC(2027, 0, 1, 0, 0), Date.UTC(2026, 2, 8, 9, 59), Date.UTC(2026, 10, 1, 9, 0)]
const spans = [0, 30_000, 5 * MINUTE, 59 * MINUTE, HOUR, 23 * HOUR, 25 * HOUR, 47 * HOUR, 49 * HOUR, 6 * DAY, 13 * DAY, 40 * DAY, 400 * DAY]

console.log(JSON.stringify({
  dayOf: instants.map(dayOf),
  undated: instants.flatMap(at => [ago(at, undefined), played(at, undefined), saved(at, undefined)]),
  ago: spans.map(span => ago(NOW - span, NOW)),
  played: spans.map(span => played(NOW - span, NOW)),
  saved: spans.map(span => saved(NOW - span, NOW)),
  // The same span, from instants a day's boundary apart in every zone: the words must not move.
  boundary: [0, 6, 12, 18].map(hour => ago(Date.UTC(2026, 10, 10, hour) - 26 * HOUR, Date.UTC(2026, 10, 10, hour))),
  within: spans.map(span => within(NOW - span, NOW, 7)),
  localHourOfTheZoneUnderTest: new Date(Date.UTC(2026, 10, 6, 0, 30)).getHours(),
}))
