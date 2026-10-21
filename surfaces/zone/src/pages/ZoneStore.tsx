import "./ZoneStore.css"
import type { SurfaceAction, SurfaceStore, SurfaceStoreEntry, SurfaceStoreGet } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef, type ReactNode } from "react"
import {
  ZONE_STORE_QUERY_MAX,
  ZONE_STORE_SEARCH_ROW,
  zoneStoreActionRow,
  zoneStoreEntryRow,
  zoneStorePlayRow,
  zoneStoreProviderRow,
  zoneStoreProviderTitle,
  zoneStoreSearch,
  type ZoneStorePage,
} from "../plan/store"
import { ZoneBanner } from "../ui/atoms/ZoneBanner"
import { ZoneCaps } from "../ui/atoms/ZoneCaps"
import { ZoneCoverWell } from "../ui/atoms/ZoneCoverWell"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZonePanel } from "../ui/atoms/ZonePanel"
import { ZoneTag, type ZoneTagItem } from "../ui/atoms/ZoneTag"
import { ZoneActionRow } from "../ui/molecules/ZoneActionRow"
import { ZoneBusyStrip } from "../ui/molecules/ZoneBusyStrip"
import { ZoneDialog } from "../ui/molecules/ZoneDialog"
import { ZoneFoot } from "../ui/molecules/ZoneFoot"
import { ZoneKeyboard, type ZoneKey, type ZoneKeyboardLayer } from "../ui/molecules/ZoneKeyboard"
import { ZoneOptionGroup, type ZoneOptionGroupRow } from "../ui/molecules/ZoneOptionGroup"
import { ZoneShell, type ZoneBand, type ZoneStrips } from "../ui/templates/ZoneShell"

/**
 * Where the person is on GET GAMES, as the store model holds it. The page
 * may not import src/state, so it declares what it reads;
 * test/zone-store.test.tsx holds the model assignable to it.
 */
export interface ZoneStoreView {
  readonly page: ZoneStorePage
  readonly provider: string | undefined
  readonly keyboard:
    | { readonly draft: string; readonly caret: number; readonly layer: ZoneKeyboardLayer; readonly key: string }
    | undefined
  readonly focused: string | undefined
  readonly dialog: number
}

/** The selector for whatever the props say holds the cursor. */
const FOCUSED = "button[data-focused='on'], button[data-focused='true'], .zone-option-row[data-focused] > button"

/** The first button of a question that can take the cursor, from `at`. */
const enabledFrom = (actions: readonly SurfaceAction[], at: number): number => {
  if (actions[at]?.enabled === true) return at
  return Math.max(0, actions.findIndex(action => action.enabled))
}

/**
 * GET GAMES: a game from a provider, found, got and played, on one page
 * over GAMES. It has no tabs, as RUN WITH has none; the banner names it and
 * B walks back out: the keyboard, the entry, the results, the providers,
 * then GAMES.
 *
 *  - Providers: one row per provider Korri lists, OPEN on each.
 *  - A provider: SEARCH with what was asked, and Korri's results, one row
 *    per game with the system it runs on. A provider not asked yet opens
 *    on the keyboard. Korri's search is SEARCHING on the busy strip, its
 *    trouble a dialog with its message and its RETRY, nothing found
 *    NOTHING FOUND.
 *  - An entry: its title beside the banner and its system as a tag, the
 *    provider's description, then where getting it stands: Korri's actions
 *    as buttons (GET), NOT AVAILABLE with Korri's reason, Korri's question
 *    as a dialog with its own buttons, the busy strip with its meter and
 *    CANCEL (QUEUED over it while the download waits its turn), IN LIBRARY
 *    with PLAY, or Korri's problem as a dialog with RETRY.
 *
 * Every word on the page that is not a label is Korri's, verbatim. Every
 * action goes back by its id through `onAction`; PLAY goes out as the row
 * `play:<gameId>`. Korri draws no provider art and Zone invents none.
 *
 * A shape: the store and the view come in as props; focus and presses go
 * out through callbacks. When DOM focus has nowhere to be (the control it
 * was on went away, or the page just opened), the page puts it on the
 * control the view names, else on its first control.
 */
export function ZoneStore({
  store,
  view,
  strips,
  band,
  clock,
  build,
  onRowFocus,
  onRowPress,
  onKeyFocus,
  onKey,
  onDialogFocus,
  onAction,
}: {
  readonly store: SurfaceStore
  readonly view: ZoneStoreView
  readonly strips?: ZoneStrips
  readonly band?: ZoneBand
  readonly clock?: string
  readonly build?: string
  readonly onRowFocus?: (rowId: string) => void
  readonly onRowPress?: (rowId: string) => void
  readonly onKeyFocus?: (id: string) => void
  readonly onKey?: (key: ZoneKey) => void
  readonly onDialogFocus?: (index: number) => void
  readonly onAction?: (actionId: string) => void
}) {
  const body = useRef<HTMLDivElement | null>(null)
  const press = onRowPress ?? (() => undefined)
  const run = onAction ?? (() => undefined)

  // Follow the view with DOM focus, but never take it from anywhere real:
  // only when it has fallen to the document (its control went away).
  useLayoutEffect(() => {
    const box = body.current
    if (box === null || box.closest("[inert]") !== null) return
    const active = document.activeElement
    if (active !== null && active !== document.body && active.isConnected) return
    const key = view.keyboard?.key
    const keyed = key === undefined ? undefined
      : [...box.querySelectorAll<HTMLButtonElement>("[data-key-id]")].find(button => button.dataset["keyId"] === key)
    const target = keyed ?? box.querySelector<HTMLButtonElement>(FOCUSED)
      ?? box.querySelector<HTMLButtonElement>("button:not([disabled])")
    target?.focus({ preventScroll: true })
  })

  const where = whereTitle(store, view)
  const tags = headTags(store, view)

  return (
    <ZoneShell
      band={band}
      foot={clock === undefined && build === undefined ? undefined : <ZoneFoot build={build} clock={clock} />}
      hintBack={<ZoneKeycap button="B" word="Back" />}
      hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
      strips={strips}
    >
      <div className="zone-store" data-page={view.page}>
        <div className="zone-store-head">
          <ZoneBanner>Get games</ZoneBanner>
          {where === undefined ? null : (
            <span className="zone-store-where">
              <ZonePanel>
                <ZoneCaps>{where}</ZoneCaps>
              </ZonePanel>
            </span>
          )}
          {tags.map(tag => (
            <span className="zone-store-tag" key={tag.label}>
              <ZoneTag kind={tag.kind} label={tag.label} />
            </span>
          ))}
        </div>
        <div className="zone-store-scroll" data-block-exit="true" ref={body}>
          {view.keyboard !== undefined ? (
            <div className="zone-store-keys">
              <ZoneKeyboard
                caret={view.keyboard.caret}
                draft={view.keyboard.draft}
                focusedKey={view.keyboard.key}
                layer={view.keyboard.layer}
                maxLength={ZONE_STORE_QUERY_MAX}
                onFocusKey={onKeyFocus}
                onKey={onKey}
              />
            </div>
          ) : (
            <div className="zone-store-rows">
              {view.page === "providers" ? providersBody(store, view, press, run, onRowFocus, onDialogFocus)
                : view.page === "results" ? resultsBody(store, view, press, run, onRowFocus, onDialogFocus)
                : entryBody(store, view, press, run, onRowFocus, onDialogFocus)}
            </div>
          )}
        </div>
      </div>
    </ZoneShell>
  )
}

type Press = (id: string) => void
type Focus = ((id: string) => void) | undefined
type FocusIndex = ((index: number) => void) | undefined

/** Beside the banner: the provider on its results, the game on its entry. */
function whereTitle(store: SurfaceStore, view: ZoneStoreView): string | undefined {
  switch (view.page) {
    case "providers":
      return undefined
    case "results":
      return zoneStoreProviderTitle(store, view.provider)
    case "entry":
      return store.details._tag === "Closed" ? undefined : store.details.entry.title
  }
}

/** The entry's system in blue, and a word for where getting it stands. */
function headTags(store: SurfaceStore, view: ZoneStoreView): readonly ZoneTagItem[] {
  if (view.page !== "entry" || store.details._tag === "Closed") return []
  const system: readonly ZoneTagItem[] = systemTag(store.details.entry)
  if (store.details._tag !== "Ready") return system
  const get = store.details.get
  // Blue for settled, white for not yet, red for stopped, as RUN WITH's
  // state tag; orange for a download, as a game card's DOWNLOAD tag.
  switch (get._tag) {
    case "Working":
      return [...system, get.queued === true ? { kind: "continue", label: "Queued" } : { kind: "download", label: "Download" }]
    case "InLibrary":
      // Korri's detail says it when it has one; the tag says it when not.
      return get.detail === undefined ? [...system, { kind: "system", label: "In library" }] : system
    case "Unavailable":
      return [...system, { kind: "remote", label: "Not available" }]
    case "Offer":
    case "Confirm":
    case "Problem":
      return system
  }
}

const systemTag = (entry: SurfaceStoreEntry): readonly ZoneTagItem[] =>
  entry.system === undefined ? [] : [{ kind: "system", label: entry.system }]

/** A word on a black panel, centred: NO PROVIDERS, NOTHING FOUND. */
const word = (text: string): ReactNode => (
  <div className="zone-store-word">
    <ZonePanel>
      <ZoneCaps>{text}</ZoneCaps>
    </ZonePanel>
  </div>
)

/** Korri's trouble: its words as the dialog's line, its actions as the buttons. */
function trouble(
  headline: string,
  message: string,
  actions: readonly SurfaceAction[],
  view: ZoneStoreView,
  run: Press,
  onDialogFocus: FocusIndex,
): ReactNode {
  return (
    <div className="zone-store-dialog">
      <ZoneDialog
        actions={actions.map(action => ({
          id: action.id,
          label: action.label,
          disabled: !action.enabled,
          destructive: action.destructive,
          onPress: () => run(action.id),
        }))}
        focused={enabledFrom(actions, view.dialog)}
        headline={headline}
        lines={[message]}
        onFocusAction={onDialogFocus}
      />
    </div>
  )
}

function providersBody(
  store: SurfaceStore,
  view: ZoneStoreView,
  press: Press,
  run: Press,
  onRowFocus: Focus,
  onDialogFocus: FocusIndex,
): ReactNode {
  const providers = store.providers
  switch (providers._tag) {
    case "Loading":
      return <ZoneBusyStrip kicker="Loading" />
    case "Problem":
      return trouble("Providers", providers.message, providers.actions, view, run, onDialogFocus)
    case "Ready":
      if (providers.providers.length === 0) return word("No providers")
      return (
        <ZoneOptionGroup
          focusedId={view.focused}
          onFocus={onRowFocus}
          onPress={press}
          rows={providers.providers.map((provider): ZoneOptionGroupRow => ({
            id: zoneStoreProviderRow(provider.id),
            label: provider.title,
            value: "Open",
            interaction: { kind: "action", actionId: provider.id },
          }))}
          title="Providers"
        />
      )
  }
}

function resultsBody(
  store: SurfaceStore,
  view: ZoneStoreView,
  press: Press,
  run: Press,
  onRowFocus: Focus,
  onDialogFocus: FocusIndex,
): ReactNode {
  const search = zoneStoreSearch(store, view.provider)
  const searchRow: ZoneOptionGroupRow = {
    id: ZONE_STORE_SEARCH_ROW,
    label: "Search for",
    value: search._tag === "Idle" ? "" : search.query,
    interaction: { kind: "text" },
  }
  return (
    <>
      <ZoneOptionGroup
        focusedId={view.focused}
        onFocus={onRowFocus}
        onPress={press}
        rows={[searchRow]}
        title="Search"
      />
      {(() => {
        switch (search._tag) {
          case "Idle":
            return null
          case "Searching":
            return <ZoneBusyStrip kicker="Searching" />
          case "Problem":
            return trouble("Search", search.message, search.actions, view, run, onDialogFocus)
          case "Ready":
            if (search.entries.length === 0) return word("Nothing found")
            return (
              <ZoneOptionGroup
                focusedId={view.focused}
                onFocus={onRowFocus}
                onPress={press}
                rows={search.entries.map((entry): ZoneOptionGroupRow => ({
                  id: zoneStoreEntryRow(entry.id),
                  label: entry.title,
                  // The system as the provider says it. When it does not say, the
                  // value is absent and Zone writes nothing in its place.
                  ...(entry.system === undefined ? {} : { value: entry.system }),
                  interaction: { kind: "action", actionId: entry.id },
                }))}
                title="Results"
              />
            )
        }
      })()}
    </>
  )
}

function entryBody(
  store: SurfaceStore,
  view: ZoneStoreView,
  press: Press,
  run: Press,
  onRowFocus: Focus,
  onDialogFocus: FocusIndex,
): ReactNode {
  const details = store.details
  if (details._tag === "Closed") return <ZoneBusyStrip kicker="Loading" />
  // Korri carries no provider art, so the well stands in: stripes and initials.
  return (
    <div className="zone-store-entry">
      <span className="zone-store-well">
        <ZoneCoverWell title={details.entry.title} />
      </span>
      <div className="zone-store-entry-text">
        {(() => {
          switch (details._tag) {
            case "Loading":
              return <ZoneBusyStrip kicker="Loading" />
            case "Problem":
              return trouble(details.entry.title, details.message, details.actions, view, run, onDialogFocus)
            case "Ready":
              return (
                <>
                  {details.description === undefined ? null : <p className="zone-store-line">{details.description}</p>}
                  {getting(details.get, view, press, run, onRowFocus, onDialogFocus)}
                </>
              )
          }
        })()}
      </div>
    </div>
  )
}

/** Where getting the entry stands, with Korri's own actions. */
function getting(
  get: SurfaceStoreGet,
  view: ZoneStoreView,
  press: Press,
  run: Press,
  onRowFocus: Focus,
  onDialogFocus: FocusIndex,
): ReactNode {
  const row = (actions: readonly { readonly id: string; readonly label: string; readonly disabled?: boolean }[]) => (
    <div className="zone-store-actions">
      <ZoneActionRow
        actions={actions}
        focused={actions.some(action => action.id === view.focused) ? view.focused : undefined}
        lit={actions.find(action => action.disabled !== true)?.id}
        onFocus={onRowFocus}
        onPress={press}
      />
    </div>
  )
  switch (get._tag) {
    case "Offer":
      return (
        <>
          {get.note === undefined ? null : <p className="zone-store-line">{get.note}</p>}
          {row(get.actions.map(action => ({
            id: zoneStoreActionRow(action.id),
            label: action.label,
            ...(action.enabled ? {} : { disabled: true }),
          })))}
        </>
      )
    case "Unavailable":
      return <p className="zone-store-line">{get.reason}</p>
    case "Confirm":
      return (
        <div className="zone-store-dialog">
          <ZoneDialog
            actions={get.actions.map(action => ({
              id: action.id,
              label: action.label,
              disabled: !action.enabled,
              destructive: action.destructive,
              onPress: () => run(action.id),
            }))}
            focused={enabledFrom(get.actions, view.dialog)}
            headline={get.kicker}
            lines={get.detail === undefined ? [] : [get.detail]}
            onFocusAction={onDialogFocus}
          />
        </div>
      )
    case "Working":
      return (
        <div className="zone-store-busy">
          <ZoneBusyStrip
            actions={get.actions}
            detail={get.detail}
            focused={enabledFrom(get.actions, view.dialog)}
            kicker={get.kicker}
            onAction={run}
            onFocusAction={onDialogFocus}
            progress={get.progress}
          />
        </div>
      )
    case "InLibrary":
      return (
        <>
          {get.detail === undefined ? null : <p className="zone-store-line">{get.detail}</p>}
          {get.gameId === undefined ? null : row([{ id: zoneStorePlayRow(get.gameId), label: "Play" }])}
        </>
      )
    case "Problem":
      return trouble(get.kicker, get.reason, get.actions, view, run, onDialogFocus)
  }
}
