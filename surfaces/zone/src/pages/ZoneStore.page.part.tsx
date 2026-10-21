import type { SurfaceStore, SurfaceStoreGet } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import {
  createStoreProgram,
  fixtureStore,
  fixtureStoreDetails,
  fixtureStoreEntries,
  fixtureStoreFresh,
  fixtureStoreGets,
  fixtureStoreManyEntries,
  fixtureStoreOpenings,
  storeCallbacks,
} from "../fixtures/fixture-store"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import { plan } from "../plan/zone-select"
import { ZoneStore } from "./ZoneStore"

export const name = "Get games"
export const note = "A game from a provider: the providers, a search and its results, one entry and where getting it stands, in Korri's words"

export const composition = {
  default: [
    { part: "src/ui/templates/ZoneShell.template.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneOptionGroup.molecule.part.tsx", state: "default" },
  ],
  Keyboard: [{ part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "default" }],
  Offer: [{ part: "src/ui/molecules/ZoneActionRow.molecule.part.tsx", state: "default" }],
  Confirm: [{ part: "src/ui/molecules/ZoneDialog.molecule.part.tsx", state: "default" }],
  Downloading: [{ part: "src/ui/molecules/ZoneBusyStrip.molecule.part.tsx", state: "default" }],
  Queued: [{ part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "default" }],
} as const

type Opening = keyof typeof fixtureStoreOpenings

/** Unexported: the real page on a live program, so focus and presses answer. */
function Live({ store, opening = "providers", share = 1 }: {
  readonly store: SurfaceStore
  readonly opening?: Opening
  readonly share?: number
}) {
  const program = useRef<ReturnType<typeof createStoreProgram> | null>(null)
  program.current ??= createStoreProgram(store, fixtureStoreOpenings[opening])
  const live = program.current
  const snapshot = useSyncExternalStore(live.subscribe, live.get)
  // The binding measures its root; a preview reads the frame once.
  const layout = plan(innerWidth * share, innerHeight)
  return (
    <ZoneStore
      band={layout.band}
      build="korri 0.4.1 · 9a1fb22"
      clock="21:47"
      store={snapshot.store}
      strips={layout.strips}
      view={snapshot.state}
      {...storeCallbacks(live.dispatch)}
    />
  )
}

const entry = (get: SurfaceStoreGet) => fixtureStore({ details: fixtureStoreDetails(get) })

/** The providers Korri lists: OPEN on each. */
export default function Providers() {
  return <Live store={fixtureStoreFresh} />
}

/** Korri is reading its providers: the busy strip, nothing to press. */
export function ProvidersLoading() {
  return <Live store={fixtureStore({ providers: { _tag: "Loading" }, search: { _tag: "Idle" }, details: { _tag: "Closed" } })} />
}

/** Korri could not list its providers: its message and its RETRY. */
export function ProvidersProblem() {
  return (
    <Live
      store={fixtureStore({
        providers: {
          _tag: "Problem",
          message: "Korri couldn't read its providers.",
          actions: [{ id: "store:1:providers-retry", label: "Retry", enabled: true }],
        },
        search: { _tag: "Idle" },
        details: { _tag: "Closed" },
      })}
    />
  )
}

/** No provider plugin is enabled here. */
export function NoProviders() {
  return <Live store={fixtureStore({ providers: { _tag: "Ready", providers: [] }, search: { _tag: "Idle" }, details: { _tag: "Closed" } })} />
}

/** A provider not asked yet opens on the keyboard. */
export function Keyboard() {
  return <Live opening="results" store={fixtureStoreFresh} />
}

/** The provider is asked: SEARCHING on the busy strip. */
export function Searching() {
  return <Live opening="results" store={fixtureStore({ search: { _tag: "Searching", providerId: "@korri:pico8", query: "cel" } })} />
}

/** The provider did not answer: Korri's message and RETRY. */
export function SearchProblem() {
  return (
    <Live
      opening="results"
      store={fixtureStore({
        search: {
          _tag: "Problem",
          providerId: "@korri:pico8",
          query: "cel",
          message: "The provider did not answer. Try again in a moment.",
          actions: [{ id: "store:2:search-retry", label: "Retry", enabled: true }],
        },
      })}
    />
  )
}

/** Korri's results: the system each runs on, a long title, one with no system. */
export function Results() {
  return <Live opening="results" store={fixtureStore()} />
}

/** More results than the screen: the list scrolls. */
export function ManyResults() {
  return (
    <Live
      opening="results"
      store={fixtureStore({ search: { _tag: "Ready", providerId: "@korri:pico8", query: "", entries: fixtureStoreManyEntries } })}
    />
  )
}

/** The provider found nothing. */
export function NothingFound() {
  return <Live opening="results" store={fixtureStore({ search: { _tag: "Ready", providerId: "@korri:pico8", query: "zelda", entries: [] } })} />
}

/** Korri is reading the entry. */
export function EntryLoading() {
  return <Live opening="entry" store={fixtureStore({ details: { _tag: "Loading", entry: fixtureStoreEntries[0]! } })} />
}

/** GET on offer. */
export function Offer() {
  return <Live opening="entry" store={entry(fixtureStoreGets.offer)} />
}

/** GET again, with Korri's note about the cancelled download. */
export function OfferAfterCancel() {
  return <Live opening="entry" store={entry(fixtureStoreGets.cancelled)} />
}

/** The provider names no system: NOT AVAILABLE and Korri's reason. */
export function Unavailable() {
  return (
    <Live
      opening="entry"
      store={fixtureStore({ details: fixtureStoreDetails(fixtureStoreGets.unavailable, fixtureStoreEntries[2]) })}
    />
  )
}

/** Korri's question: the download and its size, its own buttons. */
export function Confirm() {
  return <Live opening="entry" store={entry(fixtureStoreGets.confirm)} />
}

/** Asking the provider for the file: no position yet, CANCEL. */
export function Starting() {
  return <Live opening="entry" store={entry(fixtureStoreGets.starting)} />
}

/** The meter with its share and DONE / TOTAL, and CANCEL. */
export function Downloading() {
  return <Live opening="entry" store={entry(fixtureStoreGets.downloading)} />
}

/** No total known: the size so far and the sweeping stripes. */
export function DownloadingNoTotal() {
  return <Live opening="entry" store={entry(fixtureStoreGets.unsized)} />
}

/** Waiting behind another download: QUEUED, Korri's words, CANCEL. */
export function Queued() {
  return <Live opening="entry" store={entry(fixtureStoreGets.queued)} />
}

/** Adding it to the library: Korri's words, nothing to press. */
export function Placing() {
  return <Live opening="entry" store={entry(fixtureStoreGets.placing)} />
}

/** In the library: PLAY. */
export function InLibrary() {
  return <Live opening="entry" store={entry(fixtureStoreGets.inLibrary)} />
}

/** The download stopped: Korri's kicker, its reason, RETRY. */
export function Stopped() {
  return <Live opening="entry" store={entry(fixtureStoreGets.stopped)} />
}

/** The longest title on the narrow column, with its download under way. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Live
        opening="entry"
        share={0.36}
        store={fixtureStore({ details: fixtureStoreDetails(fixtureStoreGets.downloading, fixtureStoreEntries[1]) })}
      />
    </ZonePartFrame>
  )
}
