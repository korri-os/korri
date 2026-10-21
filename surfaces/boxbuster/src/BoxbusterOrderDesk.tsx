/**
 * The order desk by the door: getting a game from a provider by mail order.
 *
 * It is the door board (BoxbusterNotice) grown into a counter: the store's
 * blue board and yellow trim, printed in the counter's type. Four pages: the
 * catalogues (Korri's providers); the order form, typed on the desk's keys;
 * what the catalogue lists; and one entry with where getting it stands. Every
 * word and action on a page is Korri's, passed through untouched. A download
 * shows as a VCR-style counter of sixteen cells from Korri's bytes. When the
 * game is in the library, the desk hands you the tape and sends you to the
 * TV, because that is where games are played here.
 */
import "./BoxbusterOrderDesk.css"
import { useLayoutEffect, useRef } from "react"
import {
  DESK_KEYS,
  DESK_TEXT_MAX,
  type DeskAction,
  type DeskGet,
  type DeskPlace,
  type DeskStore,
  deskCells,
  deskDelete,
  deskSearchFor,
  deskType,
} from "./boxbuster-desk"

const CELLS = Array.from({ length: 16 }, (_, index) => index)

export function BoxbusterOrderDesk({
  store,
  place,
  onPlace,
  onSearch,
  onOpenEntry,
  onAction,
  onPlay,
  onLeave,
  canFetch,
}: {
  store: DeskStore
  place: DeskPlace
  onPlace: (place: DeskPlace) => void
  onSearch: (providerId: string, query: string) => void
  onOpenEntry: (entryId: string) => void
  onAction: (actionId: string) => void
  /** Take the tape to the TV. */
  onPlay: (gameId: string) => void
  onLeave: () => void
  /** Whether the tape with this id is on a shelf yet. */
  canFetch: (gameId: string) => boolean
}) {
  const board = useRef<HTMLDivElement>(null)
  const providers = store.providers._tag === "Ready" ? store.providers.providers : []
  const provider = providers.find(candidate => candidate.id === place.providerId)
  // A catalogue Korri no longer lists cannot hold its pages.
  const view = place.view !== "Providers" && provider === undefined && store.providers._tag === "Ready" ? "Providers" : place.view
  const search = deskSearchFor(store, place.providerId)

  // Focus is where the host's confirm lands: seed it on each page, and move
  // it on when the focused control goes away or goes inert.
  const seeded = useRef<string>("")
  useLayoutEffect(() => {
    const root = board.current
    if (root === null) return
    const page = `${view}|${place.providerId ?? ""}`
    const active = root.ownerDocument.activeElement
    const lost = active === null || active === root.ownerDocument.body || !active.isConnected ||
      (root.contains(active) && active.matches(":disabled"))
    if (!lost && seeded.current === page) return
    seeded.current = page
    ;(root.querySelector<HTMLButtonElement>("[data-desk-landing] button:not([disabled])") ??
      root.querySelector<HTMLButtonElement>(".boxbuster-desk-page button:not([disabled])") ??
      root.querySelector<HTMLButtonElement>("button:not([disabled])"))?.focus()
  })

  const actionButtons = (actions: readonly DeskAction[]) => actions.length === 0 ? null : (
    <div className="boxbuster-desk-actions" data-desk-landing="">
      {actions.map(action => (
        <button
          className="boxbuster-desk-button"
          disabled={!action.enabled}
          key={action.id}
          onClick={() => onAction(action.id)}
          type="button"
        >
          {action.label}
        </button>
      ))}
    </div>
  )

  const getPanel = (get: DeskGet) => {
    switch (get._tag) {
      case "Offer":
        return (
          <>
            {actionButtons(get.actions)}
            {get.note === undefined ? null : <p className="boxbuster-desk-note">{get.note}</p>}
          </>
        )
      case "Unavailable":
        return <p className="boxbuster-desk-slip">{get.reason}</p>
      case "Confirm":
        return (
          <>
            <p className="boxbuster-desk-kicker">{get.kicker}</p>
            {get.detail === undefined ? null : <p className="boxbuster-desk-note">{get.detail}</p>}
            {actionButtons(get.actions)}
          </>
        )
      case "Working": {
        const lit = get.progress === undefined ? undefined : deskCells(get.progress, CELLS.length)
        const percent = get.progress?.total === undefined || get.progress.total <= 0
          ? undefined
          : Math.max(0, Math.min(100, Math.floor((get.progress.done / get.progress.total) * 100)))
        return (
          <>
            <p className="boxbuster-desk-kicker">{get.kicker}</p>
            <span
              aria-label={get.kicker}
              className="boxbuster-desk-counter"
              data-counter={get.progress === undefined ? "clock" : lit === undefined ? "band" : "bytes"}
              role="progressbar"
              {...(percent === undefined ? {} : { "aria-valuemin": 0, "aria-valuemax": 100, "aria-valuenow": percent })}
            >
              {CELLS.map(cell => (
                <i
                  className="boxbuster-desk-cell"
                  data-cell={cell}
                  {...(lit === undefined ? {} : { "data-lit": cell < lit ? "on" : "off" })}
                  key={cell}
                  // Without a known end, one lit cell walks along the counter.
                  {...(lit === undefined ? { style: { animationDelay: `${(cell * 1.6) / CELLS.length}s` } } : {})}
                />
              ))}
            </span>
            {get.detail === undefined ? null : <p className="boxbuster-desk-note">{get.detail}</p>}
            {actionButtons(get.actions)}
          </>
        )
      }
      case "InLibrary":
        return (
          <>
            <p className="boxbuster-desk-kicker">{get.detail ?? "In your library"}</p>
            {get.gameId !== undefined && canFetch(get.gameId) ? (
              <div className="boxbuster-desk-actions" data-desk-landing="">
                <button className="boxbuster-desk-button" onClick={() => onPlay(get.gameId ?? "")} type="button">
                  Take it to the TV
                </button>
              </div>
            ) : (
              <p className="boxbuster-desk-note">It is on its way to the shelves.</p>
            )}
          </>
        )
      case "Problem":
        return (
          <>
            <p className="boxbuster-desk-kicker" data-tone="alarm">{get.kicker}</p>
            <p className="boxbuster-desk-note">{get.reason}</p>
            {actionButtons(get.actions)}
          </>
        )
    }
  }

  const page = (() => {
    if (view === "Providers") {
      if (store.providers._tag === "Loading") return <p className="boxbuster-desk-note">Finding the catalogues…</p>
      if (store.providers._tag === "Problem") {
        return (
          <>
            <p className="boxbuster-desk-note">{store.providers.message}</p>
            {actionButtons(store.providers.actions)}
          </>
        )
      }
      if (providers.length === 0) {
        return <p className="boxbuster-desk-note">No catalogue is turned on for this device. Turn a provider on in Settings to order games here.</p>
      }
      return (
        <ul className="boxbuster-desk-list" data-desk-landing="">
          {providers.map(candidate => (
            <li key={candidate.id}>
              <button
                className="boxbuster-desk-row"
                onClick={() => {
                  const searched = deskSearchFor(store, candidate.id)._tag !== "Idle"
                  onPlace({
                    view: searched ? "Results" : "Form",
                    providerId: candidate.id,
                    text: candidate.id === place.providerId ? place.text : "",
                  })
                }}
                type="button"
              >
                {candidate.title}
              </button>
            </li>
          ))}
        </ul>
      )
    }
    if (provider === undefined) return <p className="boxbuster-desk-note">Finding the catalogues…</p>
    if (view === "Form") {
      const type = (character: string) => onPlace(deskType(place, character))
      return (
        <form
          aria-label={`Order from ${provider.title}`}
          className="boxbuster-desk-form"
          onSubmit={event => event.preventDefault()}
        >
          <p className="boxbuster-desk-line" aria-live="polite">
            <span className="boxbuster-desk-line-label">Title</span>
            <span className="boxbuster-desk-line-text">{place.text === "" ? " " : place.text}</span>
            <span className="boxbuster-desk-line-count">{`${[...place.text].length}/${DESK_TEXT_MAX}`}</span>
          </p>
          <div className="boxbuster-desk-keys" data-desk-landing="">
            {DESK_KEYS.flat().map(key => (
              <button aria-label={`Write ${key}`} className="boxbuster-desk-key" key={key} onClick={() => type(key)} type="button">
                {key}
              </button>
            ))}
            <button aria-label="Write a space" className="boxbuster-desk-key" data-wide="" onClick={() => type(" ")} type="button">space</button>
            <button aria-label="Rub out" className="boxbuster-desk-key" data-wide="" onClick={() => onPlace(deskDelete(place))} type="button">rub out</button>
            <button aria-label="Clear" className="boxbuster-desk-key" data-wide="" onClick={() => onPlace({ ...place, text: "" })} type="button">clear</button>
          </div>
          <div className="boxbuster-desk-actions">
            <button
              className="boxbuster-desk-button"
              onClick={() => {
                onSearch(provider.id, place.text.trim())
                onPlace({ ...place, view: "Results" })
              }}
              type="button"
            >
              Search
            </button>
          </div>
        </form>
      )
    }
    if (view === "Results") {
      const again = (
        <div className="boxbuster-desk-actions">
          <button className="boxbuster-desk-button" onClick={() => onPlace({ ...place, view: "Form" })} type="button">
            {search._tag === "Idle" ? "Fill in the order form" : `Search again (${search.query === "" ? "anything" : search.query})`}
          </button>
        </div>
      )
      if (search._tag === "Idle") return again
      if (search._tag === "Searching") return <p className="boxbuster-desk-note">{`Asking ${provider.title} for “${search.query}”…`}</p>
      if (search._tag === "Problem") {
        return (
          <>
            <p className="boxbuster-desk-note">{search.message}</p>
            {actionButtons(search.actions)}
            {again}
          </>
        )
      }
      return (
        <>
          {search.entries.length === 0 ? (
            <p className="boxbuster-desk-note">
              {search.query === "" ? `${provider.title} lists nothing right now.` : `Nothing in ${provider.title} matches “${search.query}”.`}
            </p>
          ) : (
            <ul className="boxbuster-desk-list" data-desk-landing="">
              {search.entries.map(entry => (
                <li key={entry.id}>
                  <button
                    className="boxbuster-desk-row"
                    onClick={() => {
                      onOpenEntry(entry.id)
                      onPlace({ ...place, view: "Entry" })
                    }}
                    type="button"
                  >
                    <span className="boxbuster-desk-row-title">{entry.title}</span>
                    {entry.system === undefined ? null : <span className="boxbuster-desk-row-meta">{entry.system}</span>}
                  </button>
                </li>
              ))}
            </ul>
          )}
          {again}
        </>
      )
    }
    const details = store.details
    if (details._tag === "Closed") return <p className="boxbuster-desk-note">Pick a game from the catalogue.</p>
    return (
      <article className="boxbuster-desk-entry">
        <h2 className="boxbuster-desk-title">{details.entry.title}</h2>
        <p className="boxbuster-desk-meta">{[details.entry.system, provider.title].filter(part => part !== undefined).join(" · ")}</p>
        {details._tag === "Loading" ? (
          <p className="boxbuster-desk-note">Reading the catalogue…</p>
        ) : details._tag === "Problem" ? (
          <>
            <p className="boxbuster-desk-note">{details.message}</p>
            {actionButtons(details.actions)}
          </>
        ) : (
          <>
            {details.description === undefined ? null : <p className="boxbuster-desk-description">{details.description}</p>}
            {getPanel(details.get)}
          </>
        )}
      </article>
    )
  })()

  return (
    <div className="boxbuster-desk" data-desk-view={view} role="region" aria-label="Order desk">
      <div className="boxbuster-desk-board" ref={board}>
        <header className="boxbuster-desk-head">
          <h1 className="boxbuster-desk-sign">{view === "Providers" || provider === undefined ? "Order desk" : provider.title}</h1>
          <button className="boxbuster-desk-leave" onClick={onLeave} type="button">Back to the store</button>
        </header>
        <div className="boxbuster-desk-page">{page}</div>
      </div>
    </div>
  )
}
