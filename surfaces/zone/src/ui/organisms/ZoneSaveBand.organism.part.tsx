import { useRef, useSyncExternalStore } from "react"
import { fixtureManyRetiredKeys, fixtureRetiredKeys } from "../../fixtures/fixture-save-data"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { shortKey } from "../../plan/save-data"
import { ZoneSaveBand, type ZoneSaveBandCard } from "./ZoneSaveBand"

export const name = "Save band"
export const note = "MANIA MODE's save select: vertical save cards in a band, the focused one centred, the rest cut at the edges"

const fixed: readonly ZoneSaveBandCard[] = [
  { id: "backup", header: "Back up", body: { kind: "text", text: "This key" }, tag: { kind: "open", label: "New" } },
  { id: "switch-backup", header: "Switch", body: { kind: "text", text: "From backup" } },
  { id: "switch-bunker", header: "Switch", body: { kind: "text", text: "Bunker" } },
]
const retired = (keys: readonly string[]): readonly ZoneSaveBandCard[] => keys.map(key => ({
  id: key,
  header: "Retired",
  body: { kind: "text", text: shortKey(key) },
  tag: { kind: "remote", label: "Delete" },
}))

/** A ref-backed store so the preview's cursor answers focus; the product keeps it in src/state/identity.ts. */
function createFocusStore(initial: number) {
  let focus = initial
  const listeners = new Set<() => void>()
  return {
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    get: () => focus,
    set(next: number) {
      focus = next
      for (const listener of [...listeners]) listener()
    },
  }
}

/** Unexported: the real band around a cursor on local state. */
function Moving({ cards, initial }: { cards: readonly ZoneSaveBandCard[]; initial: number }) {
  const store = useRef<ReturnType<typeof createFocusStore> | null>(null)
  store.current ??= createFocusStore(initial)
  const focus = useSyncExternalStore(store.current.subscribe, store.current.get)
  return <ZoneSaveBand cards={cards} focusedIndex={focus} onChoose={index => store.current?.set(index)} onFocus={index => store.current?.set(index)} />
}

/** The three fixed cards and two retired keys, BACK UP focused at the centre. */
export default function Default() {
  return <Moving cards={[...fixed, ...retired(fixtureRetiredKeys)]} initial={0} />
}

/** The cursor mid-band: cards either side, cut at both edges. */
export function MidBand() {
  return <Moving cards={[...fixed, ...retired(fixtureRetiredKeys)]} initial={2} />
}

/** Fourteen retired keys, the cursor deep in them. */
export function Many() {
  return <Moving cards={[...fixed, ...retired(fixtureManyRetiredKeys)]} initial={9} />
}

/** One card: NOT AVAILABLE. */
export function One() {
  return <Moving cards={[{ id: "unavailable", header: "Identity", body: { kind: "text", text: "Not available" } }]} initial={0} />
}

/** A column: one whole card, its neighbours cut. */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Moving cards={[...fixed, ...retired(fixtureRetiredKeys)]} initial={1} />
    </ZonePartFrame>
  )
}

/** A strip: the cards as tall as the strip lets them be. */
export function WideShort() {
  return (
    <ZonePartFrame shape="wide-short">
      <Moving cards={[...fixed, ...retired(fixtureRetiredKeys)]} initial={1} />
    </ZonePartFrame>
  )
}
