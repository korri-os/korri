/**
 * Text being typed on Pico's keyboard: the characters and the keyboard's
 * layers. Plain values and pure changes, so the navigation model can hold a
 * draft and `update` can change it.
 */
export interface PicoTextDraft {
  readonly text: string
  /** The case key: letters show as capitals. */
  readonly capitals: boolean
  /** The symbols key: the character keys show every printable ASCII symbol. */
  readonly symbols: boolean
}

export const picoTextDraft = (text: string): PicoTextDraft => ({ text, capitals: false, symbols: false })

/** Add a character, unless the text is already at `maxLength` characters. */
export function picoDraftType(draft: PicoTextDraft, character: string, maxLength?: number): PicoTextDraft {
  return maxLength !== undefined && [...draft.text].length >= maxLength
    ? draft
    : { ...draft, text: draft.text + character }
}

/** Remove the last character, counting a character outside the BMP as one. */
export const picoDraftBackspace = (draft: PicoTextDraft): PicoTextDraft =>
  ({ ...draft, text: [...draft.text].slice(0, -1).join("") })
