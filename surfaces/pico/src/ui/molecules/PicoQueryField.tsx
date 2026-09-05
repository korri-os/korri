/**
 * What has been typed, with a blinking caret.
 *
 * Shows a prompt when empty rather than sitting blank, so the field reads as
 * something to type into even before the first key. The caret is the only thing
 * on this screen that moves, which is what makes it read as the live target.
 */
export function PicoQueryField({ query }: { readonly query: string }) {
  return (
    <div className="pico-query-field">
      <span aria-hidden className="pico-query-field-prompt">▸</span>
      {query === "" ? (
        <span className="pico-query-field-empty">TYPE TO FIND A GAME</span>
      ) : (
        <span className="pico-query-field-text">{query}</span>
      )}
      <span aria-hidden className="pico-query-field-caret">_</span>
    </div>
  )
}
