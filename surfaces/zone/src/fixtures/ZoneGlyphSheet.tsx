import "./ZoneGlyphSheet.css"
import { ZoneCaps, type ZoneCapsSize, type ZoneInk } from "../ui/atoms/ZoneCaps"

/** Every character Zone Caps draws, in the brief's order (REFINEMENT.md 2a). */
export const ZONE_CAPS_GLYPHS = [
  "ABCDEFGHIJKLM",
  "NOPQRSTUVWXYZ",
  "0123456789",
  "'\":.,-/%?!&+()#@…×▲",
  "ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏ",
  "ÐÑÒÓÔÕÖØÙÚÛÜÝÞ",
  "ĀĂĄĆĈĊČĎĐĒĔĖĘĚĜĞĠĢĤĦ",
  "ĨĪĬĮİĲĴĶĹĻĽĿŁŃŅŇŊŌŎŐ",
  "ŒŔŖŘŚŜŞŠŢŤŦŨŪŬŮŰŲŴŶŸŹŻŽ",
] as const

/**
 * A glyph sheet for Zone Caps: every character at one size in one ink, in
 * lines of the brief's groups, on the ground (or on a white bar for the
 * `ink` ink, which is drawn on white). Fixture code for Caliper only: the
 * product never renders it.
 */
export function ZoneGlyphSheet({ ink, size }: { readonly ink: ZoneInk; readonly size: ZoneCapsSize }) {
  return (
    <div className="zone-glyph-sheet" data-ink={ink}>
      {ZONE_CAPS_GLYPHS.flatMap(line => {
        const chunks = line.match(/.{1,10}/gu) ?? []
        return chunks.map(chunk => (
          <span className="zone-glyph-sheet-chunk" key={chunk}>
            <ZoneCaps ink={ink} size={size}>{chunk}</ZoneCaps>
          </span>
        ))
      })}
    </div>
  )
}
