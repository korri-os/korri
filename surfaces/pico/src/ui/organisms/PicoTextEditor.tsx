import "./PicoTextEditor.css"
import { useEffect, useLayoutEffect, useRef } from "react"
import type { PicoTextSettingRowView } from "../../pico-settings-view"
import type { PicoTextDraft } from "../../pico-text-draft"
import { PicoBadge } from "../atoms/PicoBadge"
import { PicoRow } from "../atoms/PicoRow"
import { PicoKeyboard, type PicoTyping } from "../molecules/PicoKeyboard"
import { PicoTextField } from "../molecules/PicoTextField"

/**
 * Change one text setting with Pico's keyboard.
 *
 * Shift's sheet is the reference: text starts from the value Korri published,
 * a secret starts empty, Save sends exactly what was typed, and Cancel sends
 * nothing. Save never sends an empty value; emptying a secret is the clear
 * action's job, and only when Korri publishes a label for it. Korri's limit
 * on length is enforced at the keys, and the count turns red when it is hit.
 *
 * Korri reports the save. While it is saving the editor says so and ignores
 * presses but keeps the cursor where it is; when Korri goes back to idle the
 * editor closes onto the new value. A refusal is shown in Korri's words with
 * the typing kept, so a second try does not start over.
 *
 * The editor holds no state. What is typed and the keyboard's layers are the
 * owner's (`draft`), and every key reports through `typing`. The owner decides
 * what a key does while Korri saves, at the length limit, and when the save is
 * done (src/state/update.ts). Opening, closing and the question before
 * clearing belong to the screen, because Back has to reach them.
 */
export function PicoTextEditor({
  row,
  group,
  draft,
  typing,
  onSave,
  onClose,
  onAskClear,
  onDismissProblem,
}: {
  readonly row: PicoTextSettingRowView
  /** The settings group the row belongs to, named in the wide layout. */
  readonly group: string
  readonly draft: PicoTextDraft
  readonly typing: PicoTyping
  readonly onSave: () => void
  readonly onClose: () => void
  readonly onAskClear: () => void
  readonly onDismissProblem: () => void
}) {
  const { sensitive, placeholder, maxLength, clearLabel } = row.control
  const { text } = draft
  const saving = row.state === "saving"
  const problem = typeof row.state === "object" ? row.state.problem : undefined
  const length = [...text].length
  const full = maxLength !== undefined && length >= maxLength

  /* The cursor starts on the first key and goes back to the row on close.
   * The row is read before the browser can drop focus from the hidden list
   * (a layout effect), and focused again after the list is shown (a passive
   * cleanup runs once the commit that shows it is done). */
  const root = useRef<HTMLFormElement>(null)
  const opener = useRef<Element | null>(null)
  useLayoutEffect(() => {
    opener.current = document.activeElement
    root.current?.querySelector<HTMLButtonElement>(".pico-text-editor-keys button")?.focus()
  }, [])
  useEffect(() => () => {
    const row = opener.current
    if (row instanceof HTMLElement && row.isConnected
      && (document.activeElement === document.body || document.activeElement === null)) {
      row.focus()
    }
  }, [])

  return (
    <form
      aria-label={`Change ${row.label}`}
      className="pico-text-editor"
      data-busy={saving ? "true" : undefined}
      onSubmit={(event) => event.preventDefault()}
      ref={root}
    >
      <div className="pico-text-editor-layout">
        <div className="pico-text-editor-field">
          <PicoTextField label={row.label} masked={sensitive} placeholder={placeholder} text={text} />
          <div className="pico-text-editor-foot">
            {row.value === undefined ? <span /> : (
              <span className="pico-text-editor-now">
                NOW <b>{row.value}</b>
              </span>
            )}
            {maxLength === undefined ? null : (
              <span className="pico-text-editor-count" data-full={full ? "true" : undefined}>
                {`${length}/${maxLength}`}
              </span>
            )}
          </div>
          {problem === undefined ? null : (
            <section aria-label="COULD NOT SAVE" className="pico-text-editor-problem">
              <span className="pico-text-editor-problem-kicker">COULD NOT SAVE</span>
              <p className="pico-text-editor-problem-reason">{problem}</p>
              <PicoRow label="OK" onPress={onDismissProblem} />
            </section>
          )}
        </div>
        <div className="pico-text-editor-keys">
          <PicoKeyboard
            capitals={draft.capitals}
            charset="text"
            onBackspace={typing.onBackspace}
            onClear={typing.onClear}
            onToggleCapitals={typing.onToggleCapitals}
            onToggleSymbols={typing.onToggleSymbols}
            onType={typing.onType}
            symbols={draft.symbols}
          />
        </div>
        <div className="pico-text-editor-heading">
          <span className="pico-text-editor-group">{group}</span>
          {row.description === undefined ? null : (
            <p className="pico-text-editor-description">{row.description}</p>
          )}
        </div>
        <div className="pico-text-editor-actions">
          <div className="pico-text-editor-action">
            <PicoRow
              detail={saving ? <PicoBadge text="SAVING" tone="info" /> : undefined}
              disabled={text.trim() === ""}
              label="SAVE"
              onPress={onSave}
            />
          </div>
          <div className="pico-text-editor-action">
            <PicoRow label="CANCEL" onPress={onClose} />
          </div>
          {sensitive && clearLabel !== undefined ? (
            <div className="pico-text-editor-action" data-role="clear">
              <PicoRow danger label={clearLabel.toUpperCase()} onPress={onAskClear} />
            </div>
          ) : null}
        </div>
      </div>
    </form>
  )
}
