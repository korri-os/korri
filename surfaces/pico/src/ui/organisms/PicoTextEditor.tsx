import "./PicoTextEditor.css"
import { useEffect, useLayoutEffect, useRef, useState } from "react"
import type { PicoTextSettingRowView } from "../../pico-settings-view"
import { PicoBadge } from "../atoms/PicoBadge"
import { PicoRow } from "../atoms/PicoRow"
import { PicoKeyboard } from "../molecules/PicoKeyboard"
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
 * The editor owns only what is typed. Opening, closing and the question
 * before clearing belong to the screen, because Back has to reach them.
 */
export function PicoTextEditor({
  row,
  group,
  onSave,
  onClose,
  onAskClear,
  onDismissProblem,
}: {
  readonly row: PicoTextSettingRowView
  /** The settings group the row belongs to, named in the wide layout. */
  readonly group: string
  readonly onSave: (value: string) => void
  readonly onClose: () => void
  readonly onAskClear: () => void
  readonly onDismissProblem: () => void
}) {
  const { sensitive, placeholder, maxLength, clearLabel } = row.control
  /* Seeded once: a republished model must not throw away what is typed. */
  const [text, setText] = useState(sensitive ? "" : (row.value ?? ""))
  const saving = row.state === "saving"
  const problem = typeof row.state === "object" ? row.state.problem : undefined
  const length = [...text].length
  const full = maxLength !== undefined && length >= maxLength

  /* Korri saved: it said Saving, and now it is idle again. */
  const wasSaving = useRef(false)
  useEffect(() => {
    if (saving) {
      wasSaving.current = true
    } else if (wasSaving.current) {
      wasSaving.current = false
      if (row.state === "idle") onClose()
    }
  }, [saving, row.state, onClose])

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

  const type = (character: string) => {
    if (saving) return
    setText((current) => (maxLength !== undefined && [...current].length >= maxLength ? current : current + character))
  }

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
            charset="text"
            onBackspace={() => {
              if (!saving) setText((current) => [...current].slice(0, -1).join(""))
            }}
            onClear={() => {
              if (!saving) setText("")
            }}
            onType={type}
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
              onPress={() => {
                if (!saving) onSave(text)
              }}
            />
          </div>
          <div className="pico-text-editor-action">
            <PicoRow label="CANCEL" onPress={onClose} />
          </div>
          {sensitive && clearLabel !== undefined ? (
            <div className="pico-text-editor-action" data-role="clear">
              <PicoRow danger label={clearLabel.toUpperCase()} onPress={() => {
                if (!saving) onAskClear()
              }} />
            </div>
          ) : null}
        </div>
      </div>
    </form>
  )
}
