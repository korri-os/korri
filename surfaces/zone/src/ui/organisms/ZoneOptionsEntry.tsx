import "./ZoneOptionsEntry.css"
import { useEffect, useRef, type FocusEvent } from "react"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZonePanel } from "../atoms/ZonePanel"
import { ZoneButton } from "../atoms/ZoneButton"
import { ZoneKeyboard, type ZoneKey, type ZoneKeyboardLayer } from "../molecules/ZoneKeyboard"

/** The focus id of the clear key, beside the keyboard's own key ids. */
export const ZONE_ENTRY_CLEAR = "zone:clear"

const LAYER_KEY_IDS: Record<string, string> = {
  abc: "layer:letters",
  ABC: "layer:shift",
  "#?!": "layer:symbols",
  "://": "layer:uri",
}

/** A key's id from the button the host focused, as ZoneKeyboard names it. */
function keyIdOf(button: HTMLElement): string | undefined {
  const text = button.textContent ?? ""
  switch (button.dataset.role) {
    case "layer":
      return LAYER_KEY_IDS[text]
    case "space":
      return "space"
    case "wide":
      return text.toLowerCase()
    default:
      return text === "" ? undefined : text
  }
}

/**
 * What is being typed, over the OPTIONS page: a black heading naming it (a
 * setting's label, PASSWORD and the network's name, NETWORK NAME), the
 * keyboard panel under it, and, for a secret Korri can clear, one more
 * button with Korri's clear label.
 *
 * The heading's name keeps its own case: an SSID is data. The draft, the
 * caret and the layer are props; every key reports through `onKey`. The
 * host moves DOM focus between keys; the entry reports where it landed
 * (`onFocusKey`) so the pink underline follows. On opening, the cursor
 * lands on the first key.
 */
export function ZoneOptionsEntry({
  title,
  name,
  draft,
  caret,
  secret,
  layer,
  focusedKey,
  maxLength,
  clearLabel,
  onKey,
  onFocusKey,
  onClear,
}: {
  readonly title: string
  readonly name?: string
  readonly draft: string
  readonly caret: number
  readonly secret: boolean
  readonly layer: ZoneKeyboardLayer
  readonly focusedKey?: string
  readonly maxLength?: number
  /** Korri's words for clearing a saved secret. */
  readonly clearLabel?: string
  readonly onKey: (key: ZoneKey) => void
  readonly onFocusKey?: (keyId: string) => void
  readonly onClear?: () => void
}) {
  const keys = useRef<HTMLDivElement | null>(null)
  useEffect(() => {
    const box = keys.current
    if (box === null || box.contains(document.activeElement)) return
    box.querySelector<HTMLButtonElement>("button")?.focus({ preventScroll: true })
  }, [])

  const followFocus = (event: FocusEvent<HTMLDivElement>) => {
    if (onFocusKey === undefined || !(event.target instanceof HTMLElement)) return
    const id = keyIdOf(event.target)
    if (id !== undefined) onFocusKey(id)
  }

  return (
    <div aria-label={title} className="zone-options-entry" role="dialog">
      <div className="zone-options-entry-head">
        <ZonePanel>
          <span className="zone-options-entry-title">
            <ZoneCaps>{title}</ZoneCaps>
            {name === undefined ? null : <ZoneCaps verbatim>{name}</ZoneCaps>}
          </span>
        </ZonePanel>
        {clearLabel === undefined || onClear === undefined ? null : (
          <span className="zone-options-entry-clear">
            <ZoneButton
              focused={focusedKey === ZONE_ENTRY_CLEAR}
              label={clearLabel}
              onFocus={onFocusKey === undefined ? undefined : () => onFocusKey(ZONE_ENTRY_CLEAR)}
              onPress={onClear}
            />
          </span>
        )}
      </div>
      <div className="zone-options-entry-keys" onFocus={followFocus} ref={keys}>
        <ZoneKeyboard
          caret={caret}
          draft={draft}
          focusedKey={focusedKey}
          layer={layer}
          maxLength={maxLength}
          onKey={onKey}
          secret={secret}
        />
      </div>
    </div>
  )
}
