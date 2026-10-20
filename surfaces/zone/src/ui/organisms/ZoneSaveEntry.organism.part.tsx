import { useRef, useSyncExternalStore } from "react"
import { fixtureBunkerUri } from "../../fixtures/fixture-save-data"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { type ZoneKey, type ZoneKeyboardLayer, zoneKeyId } from "../molecules/ZoneKeyboard"
import { ZoneSaveEntry } from "./ZoneSaveEntry"

export const name = "Save entry"
export const note = "One keyboard step of an identity flow: a caption naming what is typed, the keyboard under it"

interface Draft {
  readonly text: string
  readonly layer: ZoneKeyboardLayer
  readonly focusedKey: string
}

/** A ref-backed store so the preview answers its keys; the product keeps the draft in src/state/identity.ts. */
function createDraftStore(initial: Draft) {
  let state = initial
  const listeners = new Set<() => void>()
  const set = (next: Draft) => {
    state = next
    for (const listener of [...listeners]) listener()
  }
  return {
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    get: () => state,
    focus: (focusedKey: string) => set({ ...state, focusedKey }),
    press(key: ZoneKey) {
      const focusedKey = zoneKeyId(key)
      switch (key.kind) {
        case "char":
          return set({ ...state, text: state.text + key.char, focusedKey })
        case "space":
          return set({ ...state, text: `${state.text} `, focusedKey })
        case "delete":
          return set({ ...state, text: state.text.slice(0, -1), focusedKey })
        case "layer":
          return set({ ...state, layer: key.layer, focusedKey })
        case "done":
          return set({ ...state, focusedKey })
      }
    },
  }
}

/** Unexported: the real entry around a draft on local state. */
function Typing({ caption, initial, secret = false }: { caption: string; initial: Draft; secret?: boolean }) {
  const store = useRef<ReturnType<typeof createDraftStore> | null>(null)
  store.current ??= createDraftStore(initial)
  const state = useSyncExternalStore(store.current.subscribe, store.current.get)
  return (
    <ZoneSaveEntry
      caption={caption}
      caret={state.text.length}
      draft={state.text}
      focusedKey={state.focusedKey}
      layer={state.layer}
      onFocusKey={id => store.current?.focus(id)}
      onKey={key => store.current?.press(key)}
      secret={secret}
    />
  )
}

/** A password: dots, never the characters. */
export default function Password() {
  return <Typing caption="Password" initial={{ text: "hummingbird", layer: "letters", focusedKey: "d" }} secret />
}

/** The bunker URI on the URI layer, its prefix typed. */
export function Bunker() {
  return <Typing caption="Bunker" initial={{ text: "bunker://", layer: "uri", focusedKey: "1" }} />
}

/** A full bunker URI: the slab scrolls to keep the caret in view. */
export function LongUri() {
  return <Typing caption="Bunker" initial={{ text: fixtureBunkerUri, layer: "uri", focusedKey: "done" }} />
}

/** A column: the caption above, the keys wrapping under it. */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Typing caption="Backup" initial={{ text: "ncryptsec1qg", layer: "letters", focusedKey: "g" }} />
    </ZonePartFrame>
  )
}
