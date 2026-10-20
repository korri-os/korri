import "./ZoneKeyboard.css"
import "../../zone-motion.css"
import { useEffect, useLayoutEffect, useRef } from "react"

export type ZoneKeyboardLayer = "letters" | "shift" | "symbols" | "uri"

/** What a key does when A lands on it. */
export type ZoneKey =
  | { readonly kind: "char"; readonly char: string }
  | { readonly kind: "space" }
  | { readonly kind: "delete" }
  | { readonly kind: "done" }
  | { readonly kind: "layer"; readonly layer: ZoneKeyboardLayer }

/** The id a focused key is named by: the character itself, or the control's name. */
export function zoneKeyId(key: ZoneKey): string {
  switch (key.kind) {
    case "char":
      return key.char
    case "space":
    case "delete":
    case "done":
      return key.kind
    case "layer":
      return `layer:${key.layer}`
  }
}

export const ZONE_KEYBOARD_LAYERS: readonly ZoneKeyboardLayer[] = ["letters", "shift", "symbols", "uri"]

/** Four rows of characters per layer. Digits head every layer. */
const ROWS: Record<ZoneKeyboardLayer, readonly string[]> = {
  letters: ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"],
  shift: ["1234567890", "QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"],
  symbols: ["1234567890", "!@#$%^&*()", "-_=+[]{}\\|", ";:'\",.<>/?~`"],
  uri: ["1234567890", "qwertyuiop:/", "asdfghjkl.-_", "zxcvbnm?=&@"],
}

/** The glyphs on the layer keys: what the layer types, not its name. */
const LAYER_LABELS: Record<ZoneKeyboardLayer, string> = {
  letters: "abc",
  shift: "ABC",
  symbols: "#?!",
  uri: "://",
}

/** The dot that stands for a secret character. */
const DOT = "•"

/**
 * The keyboard panel: Mania's slanted blue panel rising from the foot, a
 * white slab with the typed string and a red caret, four rows of black
 * slanted keys, and a control row with the layer switch, SPACE, DELETE and
 * DONE. The focused key wears the pink underline.
 *
 * The draft and the caret are props; every key reports through `onKey`.
 * The slab scrolls sideways to keep the caret in view, so a forty-character
 * identity string is typed in the same panel. `secret` shows one dot per
 * character and never the characters.
 *
 * Rows wrap before a key gets too small to read, so no key is ever lost at
 * a narrow width. Focus is a prop: `focusedKey` is a key's id. When the
 * host moves DOM focus onto a key, `onFocusKey` reports that key's id; a
 * page that moves DOM focus to a key finds it by `data-key-id`.
 *
 * The parallelograms and the underline are drawn here; they become
 * ZonePanel and ZoneUnderline after the merge.
 */
export function ZoneKeyboard({
  draft,
  caret,
  secret = false,
  layer,
  focusedKey,
  maxLength,
  onKey,
  onFocusKey,
}: {
  readonly draft: string
  /** The insertion point: 0 to draft.length. */
  readonly caret: number
  readonly secret?: boolean
  readonly layer: ZoneKeyboardLayer
  /** The focused key's id, as `zoneKeyId` names it. */
  readonly focusedKey?: string
  readonly maxLength?: number
  readonly onKey?: (key: ZoneKey) => void
  /** The host's cursor arrived on a key: its id, as `zoneKeyId` names it. */
  readonly onFocusKey?: (id: string) => void
}) {
  const panel = useRef<HTMLDivElement | null>(null)
  const slab = useRef<HTMLDivElement | null>(null)
  const mark = useRef<HTMLSpanElement | null>(null)
  const shown = secret ? DOT.repeat(draft.length) : draft
  const at = Math.max(0, Math.min(caret, shown.length))
  const before = shown.slice(0, at)
  const after = shown.slice(at)

  // Keep the caret in view: a measurement written back to the scroll
  // position, not state.
  useLayoutEffect(() => {
    const box = slab.current
    const caretMark = mark.current
    if (box === null || caretMark === null) return
    const room = caretMark.offsetWidth * 2
    const left = caretMark.offsetLeft
    const right = left + caretMark.offsetWidth
    if (left - room < box.scrollLeft) box.scrollLeft = Math.max(0, left - room)
    else if (right + room > box.scrollLeft + box.clientWidth) box.scrollLeft = right + room - box.clientWidth
  }, [before, after])

  // A focused key below the fold of a short panel scrolls into view.
  useEffect(() => {
    panel.current?.querySelector("[data-focused]")?.scrollIntoView({ block: "nearest" })
  }, [focusedKey, layer])

  const key = (value: ZoneKey, label: string, extra?: string) => {
    const id = zoneKeyId(value)
    return (
      <button
        className="zone-keyboard-key"
        data-current={value.kind === "layer" && value.layer === layer ? true : undefined}
        data-focused={id === focusedKey || undefined}
        data-key-id={id}
        data-role={extra}
        key={id}
        onClick={onKey === undefined ? undefined : () => onKey(value)}
        onFocus={onFocusKey === undefined ? undefined : () => onFocusKey(id)}
        type="button"
      >
        {label}
      </button>
    )
  }

  return (
    <div aria-label="Keyboard" className="zone-keyboard" data-layer={layer} ref={panel} role="group">
      <div className="zone-keyboard-slab" ref={slab}>
        <span className="zone-keyboard-draft">
          {before}
          <span aria-hidden className="zone-keyboard-caret" ref={mark} />
          {after}
        </span>
      </div>
      {maxLength !== undefined ? (
        <span className="zone-keyboard-count">{draft.length}/{maxLength}</span>
      ) : null}
      <div className="zone-keyboard-rows">
        {ROWS[layer].map((row, index) => (
          <div className="zone-keyboard-row" key={index}>
            {[...row].map((char) => key({ kind: "char", char }, char))}
          </div>
        ))}
        <div className="zone-keyboard-row">
          {ZONE_KEYBOARD_LAYERS.map((name) => key({ kind: "layer", layer: name }, LAYER_LABELS[name], "layer"))}
          {key({ kind: "space" }, "Space", "space")}
          {key({ kind: "delete" }, "Delete", "wide")}
          {key({ kind: "done" }, "Done", "wide")}
        </div>
      </div>
    </div>
  )
}
