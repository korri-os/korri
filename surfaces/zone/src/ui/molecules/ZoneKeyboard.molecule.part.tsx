import { useRef, useSyncExternalStore } from "react"
import { fixtureDraft40 } from "../../fixtures/fixture-dialogs"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneKeyboard, type ZoneKey, type ZoneKeyboardLayer, zoneKeyId } from "./ZoneKeyboard"

export const name = "Keyboard"
export const note = "The slanted panel: a white slab with a red caret over four rows of black keys and the layer switch"

interface Draft {
  readonly draft: string
  readonly caret: number
  readonly layer: ZoneKeyboardLayer
  readonly focusedKey: string
}

/** A ref-backed store so the preview answers its keys; the product keeps this in src/state. */
function createDraftStore(initial: Draft) {
  let state = initial
  const listeners = new Set<() => void>()
  return {
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    get: () => state,
    focus(focusedKey: string) {
      if (state.focusedKey === focusedKey) return
      state = { ...state, focusedKey }
      for (const listener of listeners) listener()
    },
    press(key: ZoneKey, maxLength: number | undefined) {
      const { draft, caret } = state
      let next: Draft = { ...state, focusedKey: zoneKeyId(key) }
      switch (key.kind) {
        case "char":
        case "space": {
          const char = key.kind === "space" ? " " : key.char
          if (maxLength !== undefined && draft.length >= maxLength) break
          next = { ...next, draft: draft.slice(0, caret) + char + draft.slice(caret), caret: caret + 1 }
          break
        }
        case "delete":
          if (caret > 0) next = { ...next, draft: draft.slice(0, caret - 1) + draft.slice(caret), caret: caret - 1 }
          break
        case "layer":
          next = { ...next, layer: key.layer }
          break
        case "done":
          break
      }
      state = next
      for (const listener of listeners) listener()
    },
  }
}

/** Unexported: the scenario around the real keyboard. The lit key follows the host's cursor through `onFocusKey`. */
function Typing({ initial, secret = false, maxLength }: { initial: Draft; secret?: boolean; maxLength?: number }) {
  const store = useRef<ReturnType<typeof createDraftStore> | null>(null)
  store.current ??= createDraftStore(initial)
  const state = useSyncExternalStore(store.current.subscribe, store.current.get)
  return (
    <ZoneKeyboard
      caret={state.caret}
      draft={state.draft}
      focusedKey={state.focusedKey}
      layer={state.layer}
      maxLength={maxLength}
      onFocusKey={(id) => store.current?.focus(id)}
      onKey={(key) => store.current?.press(key, maxLength)}
      secret={secret}
    />
  )
}

const typed: Draft = { draft: "mini", caret: 4, layer: "letters", focusedKey: "i" }

/** The letters layer, a short draft, the cursor on a key. */
export default function Letters() {
  return <Typing initial={typed} maxLength={24} />
}

/** The shift layer: capitals. */
export function Shift() {
  return <Typing initial={{ ...typed, layer: "shift", focusedKey: "layer:shift" }} />
}

/** The symbols layer. */
export function Symbols() {
  return <Typing initial={{ draft: "mini-2", caret: 5, layer: "symbols", focusedKey: "-" }} />
}

/** The URI layer: what a bunker address needs. */
export function Uri() {
  return <Typing initial={{ draft: "bunker://", caret: 9, layer: "uri", focusedKey: "/" }} />
}

/** A password: one dot per character, never the characters. */
export function SecretDots() {
  return <Typing initial={{ draft: "hummingbird", caret: 11, layer: "letters", focusedKey: "done" }} maxLength={63} secret />
}

/** Forty characters, the caret at the end: the slab scrolls to keep it in view. */
export function LongDraft() {
  return <Typing initial={{ draft: fixtureDraft40, caret: fixtureDraft40.length, layer: "letters", focusedKey: "done" }} maxLength={40} />
}

/** Nothing typed yet: the caret alone on the slab. */
export function Empty() {
  return <Typing initial={{ draft: "", caret: 0, layer: "letters", focusedKey: "q" }} maxLength={24} />
}

/** A column: rows wrap before a key gets too small, the panel scrolls to the focused key, and the slab scrolls to the caret. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Typing initial={{ draft: fixtureDraft40, caret: 22, layer: "letters", focusedKey: "n" }} maxLength={40} />
    </ZonePartFrame>
  )
}
