import "./ZoneSaveEntry.css"
import type { FocusEvent } from "react"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZonePanel } from "../atoms/ZonePanel"
import {
  ZONE_KEYBOARD_LAYERS,
  ZoneKeyboard,
  type ZoneKey,
  type ZoneKeyboardLayer,
} from "../molecules/ZoneKeyboard"

/** The id ZoneKeyboard names a key by, read from the key's button. */
function keyIdOf(button: HTMLElement, panel: HTMLElement): string | undefined {
  const label = button.textContent ?? ""
  switch (button.dataset["role"]) {
    case undefined:
      return label.length === 1 ? label : undefined
    case "layer": {
      const index = [...panel.querySelectorAll("button[data-role='layer']")].indexOf(button)
      const layer = ZONE_KEYBOARD_LAYERS[index]
      return layer === undefined ? undefined : `layer:${layer}`
    }
    case "space":
      return "space"
    default:
      return label.toLowerCase() === "delete" ? "delete" : label.toLowerCase() === "done" ? "done" : undefined
  }
}

/**
 * One keyboard step of an identity flow: a black caption panel naming what
 * is being typed (PASSWORD, BACKUP, BUNKER), and the keyboard panel rising
 * under it. The keyboard is the vocabulary's; this organism names the step
 * and reports where the cursor is.
 *
 * The host moves DOM focus across the keys. The keyboard draws the cursor
 * from its `focusedKey` prop and has no focus callback of its own, so the
 * entry hears focus arrive on a key (React's focus event bubbles) and
 * reports the key's id, named as ZoneKeyboard names it.
 */
export function ZoneSaveEntry({
  caption,
  draft,
  caret,
  secret = false,
  layer,
  focusedKey,
  onKey,
  onFocusKey,
}: {
  readonly caption: string
  readonly draft: string
  readonly caret: number
  readonly secret?: boolean
  readonly layer: ZoneKeyboardLayer
  readonly focusedKey?: string
  readonly onKey?: (key: ZoneKey) => void
  readonly onFocusKey?: (id: string) => void
}) {
  const heard = (event: FocusEvent<HTMLDivElement>) => {
    if (onFocusKey === undefined || !(event.target instanceof HTMLElement)) return
    const id = keyIdOf(event.target, event.currentTarget)
    if (id !== undefined) onFocusKey(id)
  }
  return (
    <div className="zone-save-entry" onFocus={heard}>
      <div className="zone-save-entry-caption">
        <ZonePanel>
          <ZoneCaps size="button">{caption}</ZoneCaps>
        </ZonePanel>
      </div>
      <div className="zone-save-entry-keys">
        <ZoneKeyboard
          caret={caret}
          draft={draft}
          focusedKey={focusedKey}
          layer={layer}
          onKey={onKey}
          secret={secret}
        />
      </div>
    </div>
  )
}
