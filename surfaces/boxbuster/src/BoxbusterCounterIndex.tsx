/**
 * The counter's index: every tape, under the aisle the store shelves it in.
 *
 * This is the fast way in. It lists the same tapes in the same order as a walk
 * through the store, so knowing where a tape sits in one tells you where it
 * sits in the other. Each tape is a real button: the host moves focus between
 * them, and confirm (a click) picks the tape up.
 */
import "./BoxbusterCounterIndex.css"
import { type CSSProperties, useId } from "react"
import type { BoxbusterAisle, CounterTape } from "./boxbuster-store-view"
import { SHELVING_ACCENTS } from "./map"

const AISLE_NAMES: Readonly<Record<BoxbusterAisle, string>> = {
  returns: "Return cart",
  newReleases: "New releases",
  staffPicks: "Staff picks",
  classics: "Classics",
}

const AISLE_ORDER: readonly BoxbusterAisle[] = [
  "returns",
  "newReleases",
  "staffPicks",
  "classics",
]

export function BoxbusterCounterIndex({
  tapes,
  standingAt,
  inert,
  onLook,
  onPickUp,
}: {
  tapes: readonly CounterTape[]
  /** The tape you stand in front of; it is where focus comes home to. */
  standingAt: string | undefined
  /** True while a tape is in hand or a problem is up: the index waits. */
  inert: boolean
  onLook: (tapeId: string) => void
  onPickUp: (tapeId: string) => void
}) {
  return (
    <nav className="boxbuster-counter-index" aria-label="Tapes" inert={inert}>
      {AISLE_ORDER.map(aisle => {
        const shelved = tapes.filter(tape => tape.aisle === aisle)
        return shelved.length === 0 ? null : (
          <BoxbusterCounterIndexAisle
            key={aisle}
            aisle={aisle}
            tapes={shelved}
            standingAt={standingAt}
            onLook={onLook}
            onPickUp={onPickUp}
          />
        )
      })}
    </nav>
  )
}

function BoxbusterCounterIndexAisle({
  aisle,
  tapes,
  standingAt,
  onLook,
  onPickUp,
}: {
  aisle: BoxbusterAisle
  tapes: readonly CounterTape[]
  standingAt: string | undefined
  onLook: (tapeId: string) => void
  onPickUp: (tapeId: string) => void
}) {
  const headingId = useId()
  // The accent is runtime data shared with the room signs in the store, so it
  // arrives as a custom property rather than a hand-copied CSS colour.
  const accent = { "--bb-aisle": SHELVING_ACCENTS[aisle] } as CSSProperties
  return (
    <section className="boxbuster-counter-index-aisle" style={accent}>
      <h3 className="boxbuster-counter-index-sign" id={headingId}>
        {AISLE_NAMES[aisle]}
      </h3>
      <ul className="boxbuster-counter-index-shelf" aria-labelledby={headingId}>
        {tapes.map(tape => (
          <li key={tape.id}>
            <button
              type="button"
              className="boxbuster-counter-index-tape"
              data-tape-id={tape.id}
              {...(tape.id === standingAt ? { "data-focus-home": "" } : {})}
              onFocus={() => onLook(tape.id)}
              onClick={() => onPickUp(tape.id)}
            >
              {tape.title}
            </button>
          </li>
        ))}
      </ul>
    </section>
  )
}
