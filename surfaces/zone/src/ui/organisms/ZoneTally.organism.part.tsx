import { useFixtureHold } from "../../fixtures/fixture-moments"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneTally } from "./ZoneTally"

export const name = "Tally"
export const note = "After a session: SPECIAL STAGE's results, TIME, PLAYS and TOTAL counting up on red bars (M5); any press skips, the next closes"

/** A 72-minute session of Hollow Knight, its eighth, 3H 12M in all: the count-up, held 900 ms in. */
export default function Counting() {
  useFixtureHold(900)
  return (
    <ZoneTally gameId="hollow" plays={8} seconds={4334} title="Hollow Knight" total={11_520} />
  )
}

/** A press skipped the count-up: every value at its end. */
export function Counted() {
  return (
    <ZoneTally counted gameId="hollow" plays={8} seconds={4334} title="Hollow Knight" total={11_520} />
  )
}

/** Korri did not publish the session's length: no TIME row. */
export function NoTime() {
  return (
    <ZoneTally counted gameId="tetris" plays={1} title="Tetris" />
  )
}

/** The longest title, under an hour in all. */
export function LongTitle() {
  return (
    <ZoneTally counted gameId="zelda" plays={3} seconds={754} title="The Legend of Zelda: Oracle of Seasons" total={2400} />
  )
}

/** Narrow-tall: the rows step to caps-1. */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneTally counted gameId="hollow" plays={8} seconds={4334} title="Hollow Knight" total={11_520} />
    </ZonePartFrame>
  )
}
