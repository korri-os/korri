import "./PicoStoreResults.css"
import type { SurfaceStoreSearch } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef } from "react"
import { picoStoreResult } from "../../pico-store-view"
import { PicoRow } from "../atoms/PicoRow"
import { PicoNotice } from "../molecules/PicoNotice"
import { PicoResultRow } from "../molecules/PicoResultRow"

/**
 * One provider's search: the SEARCH row that opens the keyboard, then what
 * Korri found, as carts on the same shelf Find uses. Searching, a provider
 * that did not answer (with Korri's Retry) and nothing found each say so in
 * the body; the SEARCH row stays, so another search is one press away.
 */
export function PicoStoreResults({
  providerTitle,
  search,
  onSearch,
  onOpen,
  onAction,
}: {
  readonly providerTitle: string
  /** The search Korri holds for this provider; Idle before the first. */
  readonly search: SurfaceStoreSearch
  readonly onSearch: () => void
  readonly onOpen: (entryId: string) => void
  readonly onAction: (actionId: string) => void
}) {
  const root = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const scope = root.current
    if (scope === null) return
    const active = scope.ownerDocument.activeElement
    if (active !== null && active !== scope.ownerDocument.body && active.isConnected && !active.matches(":disabled")) return
    // The first result when there are results, else the SEARCH row.
    ;(scope.querySelector<HTMLButtonElement>(".pico-store-results-list button") ??
      scope.querySelector<HTMLButtonElement>("button:not([disabled])"))?.focus()
  })
  const query = search._tag === "Idle" ? "" : search.query
  return (
    <div className="pico-store-results" ref={root}>
      <div className="pico-store-results-search">
        <PicoRow detail={query === "" ? "TYPE A NAME" : `“${query}”`} label="SEARCH" onPress={onSearch} />
      </div>
      <div className="pico-store-results-body">
        {search._tag === "Idle" ? (
          <p className="pico-store-results-empty">Search {providerTitle} for a game by its name.</p>
        ) : search._tag === "Searching" ? (
          <PicoNotice kicker="SEARCHING" message={`Asking ${providerTitle} for “${search.query}”.`} tone="info" />
        ) : search._tag === "Problem" ? (
          <PicoNotice
            actions={search.actions.map(action => ({ label: action.label, disabled: !action.enabled, onPress: () => onAction(action.id) }))}
            kicker="NO ANSWER"
            message={search.message}
            tone="warn"
          />
        ) : search.entries.length === 0 ? (
          <p className="pico-store-results-empty">
            <span className="pico-store-results-empty-kicker">NOTHING FOUND</span>
            {search.query === ""
              ? `${providerTitle} lists nothing right now.`
              : `Nothing on ${providerTitle} matches “${search.query}”. Try other words.`}
          </p>
        ) : (
          <ul className="pico-store-results-list">
            {search.entries.map(entry => (
              <PicoResultRow game={picoStoreResult(entry)} key={entry.id} onOpen={() => onOpen(entry.id)} />
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}
