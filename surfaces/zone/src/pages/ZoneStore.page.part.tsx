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
import { ZoneGround } from "../ui/organisms/ZoneGround"
import { ZoneStore } from "./ZoneStore"

export const name = "Get games"
export const note = "A game from a provider: the providers, a search and its results, one entry and where getting it stands, in Korri's words"

export const composition = {
  default: [
    { part: "src/ui/templates/ZoneShell.template.part.tsx", state: "default" },
    { part: "src/ui/organisms/ZoneGround.organism.part.tsx", state: "Options" },
    { part: "src/ui/molecules/ZoneOptionGroup.molecule.part.tsx", state: "default" },
  ],
  Keyboard: [{ part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "default" }],
  Offer: [{ part: "src/ui/molecules/ZoneActionRow.molecule.part.tsx", state: "default" }],
  Confirm: [{ part: "src/ui/molecules/ZoneDialog.molecule.part.tsx", state: "default" }],
  Downloading: [{ part: "src/ui/molecules/ZoneBusyStrip.molecule.part.tsx", state: "default" }],
  Queued: [{ part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "default" }],
} as const

/* The helpers Caliper hands an authored check, typed locally: Zone imports
 * nothing from Caliper (test/authoring-gate.test.ts). */
interface CheckTools {
  readonly canvas: { getByRole(role: string, options: { name: RegExp }): HTMLElement }
  readonly expect: (value: unknown) => { toBe(expected: unknown): void }
  readonly waitFor: (assertion: () => void) => Promise<void>
}

/** The art pixel an element stands on, in CSS px. */
const artPxOf = (element: Element): number =>
  Number.parseFloat(getComputedStyle(element).getPropertyValue("--zone-px")) || 1

/** The boxes of an element's own words (its text nodes' line boxes). */
function wordBoxes(element: Element): DOMRect[] {
  const boxes: DOMRect[] = []
  const walk = document.createTreeWalker(element, NodeFilter.SHOW_TEXT)
  for (let node = walk.nextNode(); node !== null; node = walk.nextNode()) {
    if ((node.textContent ?? "").trim() === "") continue
    const range = document.createRange()
    range.selectNodeContents(node)
    boxes.push(...Array.from(range.getClientRects()).filter(box => box.width > 0))
  }
  return boxes
}

/** Where a key's baseline stands: a zero-size probe on it, put back at once. */
function baselineOf(key: HTMLElement): number {
  const probe = document.createElement("i")
  probe.style.cssText = "display:inline-block;width:0;height:0;vertical-align:baseline"
  key.append(probe)
  const at = probe.getBoundingClientRect().bottom
  probe.remove()
  return at
}

/**
 * Every key whose label is not whole, read off the real keyboard in
 * Chromium, and why. A label is whole when its letters (the cap above the
 * baseline, a descender under it, and the 1-art-px drop) stand inside its
 * own key and the key stands inside the panel, unscrolled, on the screen.
 */
function keysNotWhole(keyboard: HTMLElement): string {
  const px = artPxOf(keyboard)
  const panel = keyboard.getBoundingClientRect()
  const screen = { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight }
  const problems: string[] = []
  if (keyboard.scrollHeight > keyboard.clientHeight + 1) problems.push(`panel scrolls ${keyboard.scrollHeight}/${keyboard.clientHeight}`)
  const keys = Array.from(keyboard.querySelectorAll<HTMLElement>(".zone-keyboard-key"))
  for (const key of keys) {
    const name = key.textContent ?? ""
    const box = key.getBoundingClientRect()
    const words = wordBoxes(key)
    if (words.length === 0) {
      problems.push(`${name}: no label`)
      continue
    }
    const fact = key.dataset["face"] === "fact"
    const descends = fact ? /[gjpqy]/.test(name) : /[,;Q]/.test(name)
    const baseline = baselineOf(key)
    const ink = {
      left: Math.min(...words.map(word => word.left)),
      right: Math.max(...words.map(word => word.right)),
      top: baseline - 10 * px,
      bottom: baseline + (descends ? 3 * px : 0) + px,
    }
    const inside = (outer: { left: number; top: number; right: number; bottom: number }) =>
      ink.left >= outer.left - 0.5 && ink.right <= outer.right + 0.5 && ink.top >= outer.top - 0.5 && ink.bottom <= outer.bottom + 0.5
    if (!inside(box)) problems.push(`${name}: past its key (${Math.round(ink.left - box.left)},${Math.round(ink.top - box.top)} to ${Math.round(box.right - ink.right)},${Math.round(box.bottom - ink.bottom)})`)
    else if (!inside(panel)) problems.push(`${name}: past the panel`)
    else if (!inside(screen)) problems.push(`${name}: off the screen`)
  }
  return problems.join("; ")
}

/** The keys' check: every label whole, the panel unscrolled. */
async function keysWhole({ canvas, expect, waitFor }: CheckTools): Promise<void> {
  await waitFor(() => {
    expect(keysNotWhole(canvas.getByRole("group", { name: /Keyboard/ }))).toBe("")
  })
}

/* The search keyboard is FIND's, planned from the body as FIND plans it: at
 * every size its keys stand whole and unscrolled. Copied from the GAMES
 * part's check, which proves the same plan there. */
export const checks = {
  Keyboard: {
    "every key's label is whole: no key cut, none scrolled away": (tools: CheckTools) => keysWhole(tools),
  },
  KeyboardNarrow: {
    "every key's label is whole: no key cut, none scrolled away": (tools: CheckTools) => keysWhole(tools),
  },
}

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
  // GET GAMES stands on the OPTIONS ground, as the root draws it.
  return (
    <div className="zone-theme zone-screen" data-ground="options">
      <ZoneGround />
      <ZoneStore
        band={layout.band}
        build="korri 0.4.1 · 9a1fb22"
        clock="21:47"
        store={snapshot.store}
        strips={layout.strips}
        view={snapshot.state}
        {...storeCallbacks(live.dispatch)}
      />
    </div>
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

/** The search keyboard on a narrow column (480 wide). */
export function KeyboardNarrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Live opening="results" share={0.36} store={fixtureStoreFresh} />
    </ZonePartFrame>
  )
}

/** Korri's results: the system each runs on, a long title, one with no system (its value empty). */
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
