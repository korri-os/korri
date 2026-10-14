import "./PicoTextEditor.css"
import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react"
import type { PicoTextDraft } from "../../pico-text-draft"
import { PicoRow } from "../atoms/PicoRow"
import { PicoSegments } from "../atoms/PicoSegments"
import { PicoKeyboard, type PicoTyping } from "../molecules/PicoKeyboard"
import { PicoTextField } from "../molecules/PicoTextField"

/** One action beside the keys: what it says, what it shows now, what it does. */
export interface PicoTextEditorAction {
  readonly label: string
  readonly onPress: () => void
  /** What the action shows on its right, such as SAVING. */
  readonly detail?: ReactNode
  readonly disabled?: boolean
  /** Cannot be undone, such as clearing a saved key. */
  readonly danger?: boolean
  /** On its own line under the others, where the layout has lines. */
  readonly alone?: boolean
}

/**
 * Type one value with Pico's keyboard: a text setting in Settings, a Wi-Fi
 * password, a device's name during setup.
 *
 * The caller says what the value is and what can be done with it: the
 * actions (SAVE and CANCEL in Settings, CONNECT for a network, NEXT for a
 * name), whether it is a secret, and whether that secret can be shown. A
 * secret is masked; given `reveal`, a SHOW action draws it in clear while it
 * is on. Korri's limit on length is enforced at the keys by the owner, and
 * the count turns red when it is hit.
 *
 * `busy` is Korri saving: the keys go quiet but keep the cursor where it is.
 * A `problem` is Korri's refusal in its own words, with the typing kept.
 *
 * The editor holds no state. What is typed, the keyboard's layers and
 * whether a secret shows are the owner's, and every key reports through
 * `typing`. The cursor starts on the first key, and goes back to whatever
 * opened the editor when it closes, if that is still on screen.
 */
export function PicoTextEditor({
  label,
  formLabel,
  prompt,
  heading,
  now,
  note,
  draft,
  typing,
  secret = false,
  reveal,
  placeholder,
  maxLength,
  busy = false,
  problem,
  actions,
}: {
  /** The field's name, for assistive technology. */
  readonly label: string
  /** The form's name: what is being done, such as "Change Name". */
  readonly formLabel: string
  /** A line above the field that says what to type. */
  readonly prompt?: string
  /** Where the value comes from and what it is for, in the wide layout's
   * second column: a settings group and Korri's description. */
  readonly heading?: { readonly group: string; readonly description?: string }
  /** The value saved now, under the field. */
  readonly now?: string
  /** A hint under the field, where there is room for it. */
  readonly note?: string
  readonly draft: PicoTextDraft
  readonly typing: PicoTyping
  readonly secret?: boolean
  /** A secret that may be shown: whether it is, and the SHOW key. */
  readonly reveal?: { readonly shown: boolean; readonly onToggle: () => void }
  readonly placeholder?: string
  readonly maxLength?: number
  readonly busy?: boolean
  readonly problem?: { readonly message: string; readonly onDismiss: () => void }
  readonly actions: readonly PicoTextEditorAction[]
}) {
  const { text } = draft
  const length = [...text].length
  const full = maxLength !== undefined && length >= maxLength
  const masked = secret && reveal?.shown !== true
  const all: readonly PicoTextEditorAction[] = secret && reveal !== undefined
    ? [...actions, {
        label: "SHOW",
        detail: <PicoSegments current={reveal.shown ? 1 : 0} options={["OFF", "ON"]} />,
        onPress: reveal.onToggle,
      }]
    : actions

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
      aria-label={formLabel}
      className="pico-text-editor"
      data-busy={busy ? "true" : undefined}
      onSubmit={(event) => event.preventDefault()}
      ref={root}
    >
      <div className="pico-text-editor-layout">
        <div className="pico-text-editor-field" data-prompt={prompt === undefined ? undefined : "true"}>
          {prompt === undefined ? null : <span className="pico-text-editor-prompt">{prompt}</span>}
          <PicoTextField label={label} masked={masked} placeholder={placeholder} text={text} />
          <div className="pico-text-editor-foot">
            {now !== undefined ? (
              <span className="pico-text-editor-now">
                NOW <b>{now}</b>
              </span>
            ) : note !== undefined ? (
              <span className="pico-text-editor-note">{note}</span>
            ) : <span />}
            {maxLength === undefined ? null : (
              <span className="pico-text-editor-count" data-full={full ? "true" : undefined}>
                {`${length}/${maxLength}`}
              </span>
            )}
          </div>
          {problem === undefined ? null : (
            <section aria-label="COULD NOT SAVE" className="pico-text-editor-problem">
              <span className="pico-text-editor-problem-kicker">COULD NOT SAVE</span>
              <p className="pico-text-editor-problem-reason">{problem.message}</p>
              <PicoRow label="OK" onPress={problem.onDismiss} />
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
        {heading === undefined ? null : (
          <div className="pico-text-editor-heading">
            <span className="pico-text-editor-group">{heading.group}</span>
            {heading.description === undefined ? null : (
              <p className="pico-text-editor-description">{heading.description}</p>
            )}
          </div>
        )}
        <div className="pico-text-editor-actions">
          {all.map((action) => (
            <div className="pico-text-editor-action" data-alone={action.alone === true ? "true" : undefined} key={action.label}>
              <PicoRow
                danger={action.danger}
                detail={action.detail}
                disabled={action.disabled}
                label={action.label}
                onPress={action.onPress}
              />
            </div>
          ))}
        </div>
      </div>
    </form>
  )
}
