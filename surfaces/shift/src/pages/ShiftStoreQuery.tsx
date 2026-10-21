/**
 * Shift store — the search field (molecule).
 *
 * Shift's text entry is a native input, as in its setting sheet; the finder
 * pill from legacy Shift's store carries it. Search sends what is typed to
 * the one provider shown, on the button or on Enter. The text is the page's,
 * so it survives opening an entry and coming back.
 */
import { Search } from "lucide-react"
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"

export interface ShiftStoreQueryProps {
  readonly providerTitle: string
  readonly text: string
  readonly onText: (text: string) => void
  readonly onSearch: () => void
  /** A search is in flight: the button says so and waits. */
  readonly searching?: boolean
  /** Nothing searched on this provider yet: the field is where to start. */
  readonly unsearched?: boolean
}

export function ShiftStoreQuery({ providerTitle, text, onText, onSearch, searching = false, unsearched = false }: ShiftStoreQueryProps) {
  return (
    <form
      className="shift-store-query"
      role="search"
      onSubmit={event => {
        event.preventDefault()
        if (searching) return
        // Leave the field: the keyboard's arrows and Back are the field's own
        // while it has focus, and the results are where the person goes next.
        const field = event.currentTarget.querySelector("input")
        field?.blur()
        onSearch()
      }}
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.storeFinder)}
    >
      <span className="shift-store-finder-search">
        <Search className="shift-store-finder-glyph" aria-hidden="true" />
        <input
          type="search"
          className="shift-store-finder-input shift-store-query-input"
          value={text}
          placeholder={`Search ${providerTitle}`}
          aria-label={`Search ${providerTitle}`}
          enterKeyHint="search"
          {...(unsearched ? { "data-unsearched": "" } : {})}
          onChange={event => onText(event.target.value)}
        />
      </span>
      <button type="submit" className="shift-store-retry shift-store-query-go" disabled={searching}>
        {searching ? "Searching…" : "Search"}
      </button>
    </form>
  )
}
