/**
 * The split notice (ADR 0014): the game's name and "Saves split between two
 * devices". It stays on every device that knows about the split until the
 * person picks a side, so it is a standing line in the scene, not a toast that
 * times out. It is text, not a control: the Saves destination on the rail and
 * the launch itself both lead to the choice.
 */
import { SPLIT_NOTICE } from "../../save-sync-labels"
import {
  SHIFT_DESIGN_PARTS,
  shiftDesignPartAttrs,
} from "../../shift-design-parts"

export function shiftSplitNoticeSubject(titles: readonly string[]): string {
  const [first] = titles
  if (first === undefined) return ""
  if (titles.length === 1) return first
  const more = titles.length - 1
  return `${first} and ${more} more ${more === 1 ? "game" : "games"}`
}

export function ShiftSaveNotice({ titles }: { readonly titles: readonly string[] }) {
  if (titles.length === 0) return null
  return (
    <p
      className="shift-save-notice"
      role="status"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.saveNotice)}
    >
      <span className="shift-save-notice-dot" aria-hidden />
      <span className="shift-save-notice-copy">
        <span className="shift-save-notice-game">{shiftSplitNoticeSubject(titles)}</span>
        <span className="shift-save-notice-text">{SPLIT_NOTICE}</span>
      </span>
    </p>
  )
}
