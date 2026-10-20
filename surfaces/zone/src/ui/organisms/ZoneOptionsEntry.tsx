import "./ZoneOptionsEntry.css"
import { useEffect, useRef } from "react"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZonePanel } from "../atoms/ZonePanel"
import { ZoneButton } from "../atoms/ZoneButton"
import { ZoneKeyboard, type ZoneKey, type ZoneKeyboardLayer } from "../molecules/ZoneKeyboard"
import { ZoneWarningLines } from "./ZoneWarningLines"

/** The focus id of the clear key, beside the keyboard's own key ids. */
export const ZONE_ENTRY_CLEAR = "zone:clear"

/**
 * What is being typed, over the OPTIONS page: a black heading naming it (a
 * setting's label, PASSWORD and the network's name, NETWORK NAME), the
 * keyboard panel under it, and, for a secret Korri can clear, one more
 * button with Korri's clear label. When Korri refused what was typed last
 * (a rejected Wi-Fi key), its reason stands in red under the heading.
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
  problem,
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
  /** Korri's reason the last try failed, verbatim. */
  readonly problem?: string
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
        {problem === undefined ? null : (
          <span className="zone-options-entry-problem" role="alert">
            <ZoneWarningLines lines={[problem]} />
          </span>
        )}
      </div>
      <div className="zone-options-entry-keys" ref={keys}>
        <ZoneKeyboard
          caret={caret}
          draft={draft}
          focusedKey={focusedKey}
          layer={layer}
          maxLength={maxLength}
          onFocusKey={onFocusKey}
          onKey={onKey}
          secret={secret}
        />
      </div>
    </div>
  )
}
