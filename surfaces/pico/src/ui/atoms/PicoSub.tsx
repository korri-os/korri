/**
 * The line under a title: where a game came from, or what a screen is for.
 *
 * Accent-coloured and tracked out, so it reads as a label rather than as body
 * text — legacy used it for the provenance line and the tag row, and the
 * shelf's own caption is the same idea one layer down.
 */
export function PicoSub({ text }: { readonly text: string }) {
  return <div className="pico-sub">{text}</div>
}
