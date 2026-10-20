import { useRef, useSyncExternalStore } from "react"
import { fixtureDraft40 } from "../../fixtures/fixture-dialogs"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import type { ZoneKey, ZoneKeyboardLayer } from "../molecules/ZoneKeyboard"
import { ZoneOptionsEntry } from "./ZoneOptionsEntry"

export const name = "Options entry"
export const note = "What is being typed over OPTIONS: a heading, the keyboard, and a secret's clear key"

export const composition = {
  default: [{ part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "default" }],
  SecretWithClear: [
    { part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "SecretDots" },
    { part: "src/ui/atoms/ZoneButton.atom.part.tsx", state: "default" },
  ],
  LongDraft: [{ part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "LongDraft" }],
} as const

interface Typed {
  readonly draft: string
  readonly caret: number
  readonly layer: ZoneKeyboardLayer
  readonly focusedKey?: string
}

/** A ref-backed store so the preview types; the product keeps this in src/state/options.ts. */
function createTypedStore(initial: Typed) {
  let state = initial
  const listeners = new Set<() => void>()
  const set = (next: Typed) => {
    state = next
    for (const listener of listeners) listener()
  }
  return {
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    get: () => state,
    key(key: ZoneKey) {
      const { draft, caret } = state
      if (key.kind === "char" || key.kind === "space") {
        const char = key.kind === "space" ? " " : key.char
        set({ ...state, draft: draft.slice(0, caret) + char + draft.slice(caret), caret: caret + 1 })
      } else if (key.kind === "delete" && caret > 0) {
        set({ ...state, draft: draft.slice(0, caret - 1) + draft.slice(caret), caret: caret - 1 })
      } else if (key.kind === "layer") {
        set({ ...state, layer: key.layer })
      }
    },
    focus(keyId: string) {
      set({ ...state, focusedKey: keyId })
    },
    clear() {
      set({ ...state, draft: "", caret: 0 })
    },
  }
}

/** Unexported: the entry on a local draft. */
function Typing({
  initial,
  title,
  name: entryName,
  secret = false,
  maxLength,
  clearLabel,
}: {
  readonly initial: Typed
  readonly title: string
  readonly name?: string
  readonly secret?: boolean
  readonly maxLength?: number
  readonly clearLabel?: string
}) {
  const store = useRef<ReturnType<typeof createTypedStore> | null>(null)
  store.current ??= createTypedStore(initial)
  const live = store.current
  const state = useSyncExternalStore(live.subscribe, live.get)
  return (
    <ZoneOptionsEntry
      caret={state.caret}
      clearLabel={clearLabel}
      draft={state.draft}
      focusedKey={state.focusedKey}
      layer={state.layer}
      maxLength={maxLength}
      name={entryName}
      onClear={live.clear}
      onFocusKey={live.focus}
      onKey={live.key}
      secret={secret}
      title={title}
    />
  )
}

/** A text setting: its label over the keyboard, Korri's value as the draft. */
export default function Text() {
  return <Typing initial={{ draft: "mini", caret: 4, layer: "letters" }} maxLength={64} title="Name" />
}

/** A secret Korri can clear: dots, and the clear key beside the heading. */
export function SecretWithClear() {
  return (
    <Typing
      clearLabel="Clear saved key"
      initial={{ draft: "sgdb-4f2a", caret: 9, layer: "letters" }}
      maxLength={256}
      secret
      title="SteamGridDB API key"
    />
  )
}

/** A network's password: PASSWORD and the SSID in its own case. */
export function WifiPassword() {
  return <Typing initial={{ draft: "", caret: 0, layer: "letters" }} maxLength={63} name="Attic 5G" secret title="Password" />
}

/** A hidden network's name, typed in clear. */
export function HiddenName() {
  return <Typing initial={{ draft: "attic-ap", caret: 8, layer: "letters" }} maxLength={32} title="Network name" />
}

/** Forty characters: the slab scrolls to the caret. */
export function LongDraft() {
  return <Typing initial={{ draft: fixtureDraft40, caret: 40, layer: "symbols" }} maxLength={64} title="Name" />
}

/** A narrow column: the clear key drops under the heading, the keys wrap. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Typing
        clearLabel="Clear saved key"
        initial={{ draft: "sgdb", caret: 4, layer: "letters" }}
        secret
        title="SteamGridDB API key"
      />
    </ZonePartFrame>
  )
}
