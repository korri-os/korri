/**
 * Shift store — getting games from a provider (page).
 *
 * Three views, one Back each: the providers Korri lists; one provider's
 * search and its results; one entry and where getting it stands. The person's
 * place (which view, which provider, what is typed) is the caller's, so the
 * store opens where it was left. Every fact is Korri's: the lists, the entry
 * and each action come from `model.store`, and a download keeps going while
 * the person looks elsewhere.
 */
import type { SurfaceStore } from "@contracts/surface/korri-surface"
import { useEffect, useRef } from "react"
import { useSurfaceAction } from "../host/surface-host"
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftStoreBrowseTile } from "./ShiftStoreBrowseTile"
import { ShiftStoreDetail } from "./ShiftStoreDetail"
import { ShiftStoreEmpty } from "./ShiftStoreEmpty"
import { ShiftStoreIndexRow } from "./ShiftStoreIndexRow"
import { ShiftStoreQuery } from "./ShiftStoreQuery"

/** Where the person is in the store. Shift's, never Korri's. */
export interface ShiftStorePlace {
  readonly view: "providers" | "results" | "detail"
  readonly providerId?: string
  readonly text: string
}

export const SHIFT_STORE_START: ShiftStorePlace = { view: "providers", text: "" }

export interface ShiftStoreProps {
  readonly store: SurfaceStore
  readonly place: ShiftStorePlace
  readonly onPlace: (place: ShiftStorePlace) => void
  readonly onSearch: (providerId: string, query: string) => void
  readonly onOpenEntry: (entryId: string) => void
  readonly onAction: (actionId: string) => void
  readonly onPlay: (gameId: string) => void
  readonly onClose: () => void
}

export function ShiftStore({ store, place, onPlace, onSearch, onOpenEntry, onAction, onPlay, onClose }: ShiftStoreProps) {
  const root = useRef<HTMLDivElement>(null)
  const providers = store.providers._tag === "Ready" ? store.providers.providers : []
  const provider = providers.find(candidate => candidate.id === place.providerId)
  // A provider Korri no longer lists cannot hold the person on its page.
  const view = place.view !== "providers" && provider === undefined && store.providers._tag === "Ready" ? "providers" : place.view

  useSurfaceAction("back", () => {
    if (view === "detail") onPlace({ ...place, view: "results" })
    else if (view === "results") onPlace({ ...place, view: "providers" })
    else onClose()
  })

  // Each view lands focus on its first control, so confirm acts at once.
  useEffect(() => {
    const scope = root.current
    if (scope === null) return
    const active = scope.ownerDocument.activeElement
    if (active !== null && active !== scope.ownerDocument.body && scope.contains(active) && active.isConnected) return
    scope.querySelector<HTMLElement>("[data-shift-store-landing] button:not([disabled]), [data-shift-store-landing] input, button:not([disabled])")
      ?.focus({ preventScroll: true })
  })

  const search = store.search
  const shownSearch = search._tag !== "Idle" && search.providerId === place.providerId ? search : undefined

  if (view === "detail" && store.details._tag !== "Closed") {
    return (
      <div ref={root} className="shift-store-page" data-shift-store data-shift-store-view="detail">
        <ShiftStoreDetail
          details={store.details}
          {...(provider === undefined ? {} : { providerTitle: provider.title })}
          onAction={onAction}
          onPlay={onPlay}
        />
      </div>
    )
  }

  return (
    <div
      ref={root}
      data-shift-store
      data-shift-store-view={view}
      className="shift-store intrinsic"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.store)}
    >
      <header className="shift-store-top">
        <h2 className="shift-store-heading">{view === "results" && provider !== undefined ? provider.title : "Get games"}</h2>
      </header>
      {view === "providers" ? (
        store.providers._tag === "Loading" ? (
          <ShiftStoreEmpty message="Looking for providers…" />
        ) : store.providers._tag === "Problem" ? (
          <div className="shift-store-status" data-shift-store-landing>
            <ShiftStoreEmpty message={store.providers.message} />
            {store.providers.actions.map(action => (
              <button key={action.id} type="button" className="shift-store-retry" onClick={() => onAction(action.id)}>
                {action.label}
              </button>
            ))}
          </div>
        ) : providers.length === 0 ? (
          <ShiftStoreEmpty message="No provider is turned on for this device. Turn one on in Settings to get games here." />
        ) : (
          <div className="shift-store-index-rows" data-shift-store-landing>
            {providers.map(candidate => (
              <ShiftStoreIndexRow
                key={candidate.id}
                id={candidate.id}
                title={candidate.title}
                onOpen={id => onPlace({ view: "results", providerId: id, text: id === place.providerId ? place.text : "" })}
              />
            ))}
          </div>
        )
      ) : provider === undefined ? (
        <ShiftStoreEmpty message="Looking for providers…" />
      ) : (
        <>
          <div data-shift-store-landing>
            <ShiftStoreQuery
              providerTitle={provider.title}
              text={place.text}
              onText={text => onPlace({ ...place, text })}
              onSearch={() => onSearch(provider.id, place.text.trim())}
              searching={shownSearch?._tag === "Searching"}
            />
          </div>
          {shownSearch === undefined ? (
            <ShiftStoreEmpty message={`Type what to look for, then search ${provider.title}.`} />
          ) : shownSearch._tag === "Searching" ? (
            <ShiftStoreEmpty message="Searching…" />
          ) : shownSearch._tag === "Problem" ? (
            <div className="shift-store-status">
              <ShiftStoreEmpty message={shownSearch.message} />
              {shownSearch.actions.map(action => (
                <button key={action.id} type="button" className="shift-store-retry" onClick={() => onAction(action.id)}>
                  {action.label}
                </button>
              ))}
            </div>
          ) : shownSearch.entries.length === 0 ? (
            <ShiftStoreEmpty message={shownSearch.query === "" ? `${provider.title} lists nothing right now.` : `Nothing on ${provider.title} matches “${shownSearch.query}”. Try other words.`} />
          ) : (
            <div className="shift-store-tiles">
              {shownSearch.entries.map(entry => (
                <ShiftStoreBrowseTile
                  key={entry.id}
                  entry={entry}
                  onOpen={id => {
                    onOpenEntry(id)
                    onPlace({ ...place, view: "detail" })
                  }}
                />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  )
}
