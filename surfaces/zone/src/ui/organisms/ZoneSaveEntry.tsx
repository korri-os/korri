import "./ZoneSaveEntry.css"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZonePanel } from "../atoms/ZonePanel"
import {
  ZoneKeyboard,
  type ZoneKey,
  type ZoneKeyboardLayer,
} from "../molecules/ZoneKeyboard"

/**
 * One keyboard step of an identity flow: a black caption panel naming what
 * is being typed (PASSWORD, BACKUP, BUNKER), and the keyboard panel rising
 * under it. The keyboard is the vocabulary's; this organism names the step
 * and reports where the cursor is.
 *
 * The host moves DOM focus across the keys. The keyboard draws the cursor
 * from its `focusedKey` prop and reports a focused key's id through its
 * own `onFocusKey`, which the entry hands on.
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
  return (
    <div className="zone-save-entry">
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
          onFocusKey={onFocusKey}
          onKey={onKey}
          secret={secret}
        />
      </div>
    </div>
  )
}
