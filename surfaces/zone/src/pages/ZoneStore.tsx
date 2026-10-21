import "./ZoneStore.css"
import type { SurfaceAction, SurfaceStore, SurfaceStoreGet } from "@contracts/surface/korri-surface"
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
import { useZoneBandWedge } from "../use-zone-band-wedge"
import { useZoneFindPlan } from "../use-zone-find-plan"
import { useZoneFit } from "../use-zone-fit"
import { useZoneOptionClips } from "../use-zone-option-row"

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

/** Korri's question or trouble, as the dialog layer draws it. */
interface StoreDialog {
  readonly headline: string
  /** The headline is Korri's (its kicker, an entry's title), not Zone's word. */
  readonly korri: boolean
  readonly line: string | undefined
  readonly actions: readonly SurfaceAction[]
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
 * over GAMES. It stands as RUN WITH does: the OPTIONS ground, the banner
 * where the tabs stand on every other page, a head row naming where the
 * person is, and OPTIONS' sky band with its black wedge holding the rows.
 * B walks back out: the keyboard, the entry, the results, the providers,
 * then GAMES.
 *
 *  - Providers: one row per provider Korri lists, OPEN on each.
 *  - A provider: SEARCH FOR with what was asked, and Korri's results, one
 *    row per game with the system it runs on (nothing when the provider
 *    does not say). A provider not asked yet opens on the keyboard, FIND's
 *    keyboard planned from the body as FIND plans it. SEARCHING is the busy
 *    strip, Korri's trouble a dialog with its message and RETRY, nothing
 *    found NOTHING FOUND.
 *  - An entry: its title in the head with its system as a tag, a stand-in
 *    well (the treaty carries no provider art), then Korri's words on black
 *    panels in Jersey 10 as written, and where getting it stands: Korri's
 *    actions as buttons (GET), Korri's reason (NOT AVAILABLE in the head),
 *    PLAY once it is in the library, Korri's question or trouble as the sky
 *    dialog over the band, and work under way as the busy strip at the
 *    body's foot with its meter and CANCEL (QUEUED in the head while it
 *    waits its turn).
 *
 * Every sentence and name on the page is Korri's, in the look's face for
 * Korri's words. Every action goes back by its id through `onAction`; PLAY
 * goes out as the row `play:<gameId>`.
 *
 * A shape: the store and the view come in as props; focus and presses go
 * out through callbacks. When DOM focus has nowhere real to be (its control
 * went away or went inert, or the page just opened), the page puts it on
 * the control the view names, else on the first one it can.
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
  const root = useRef<HTMLDivElement | null>(null)
  const body = useRef<HTMLDivElement | null>(null)
  const keysBox = useRef<HTMLDivElement | null>(null)
  const head = useRef<HTMLDivElement | null>(null)
  const title = useRef<HTMLSpanElement | null>(null)
  const rowBand = useRef<HTMLDivElement | null>(null)
  const list = useRef<HTMLDivElement | null>(null)
  const press = onRowPress ?? (() => undefined)
  const run = onAction ?? (() => undefined)

  const where = whereTitle(store, view)
  const tags = headTags(store, view)
  const dialog = dialogOf(store, view)
  const typing = view.keyboard !== undefined
  const working = workingOf(store, view)
  const layer = dialog !== undefined ? "dialog" : typing ? "keyboard" : "none"

  useZoneBandWedge(rowBand)
  useZoneFit(head, [title], where ?? "")
  useZoneOptionClips(list, `${view.page}:${store.details._tag === "Ready" ? store.details.get._tag : store.details._tag}`)
  // FIND's keyboard plan: the band yields, then the keys' spacing, then the
  // band hides, then the keys' height; no key is cut or scrolled.
  useZoneFindPlan(root, body, keysBox, typing ? "body" : undefined)

  // Follow the view with DOM focus, but never take it from anywhere real:
  // only when it has fallen to the document or into something inert.
  useLayoutEffect(() => {
    const box = root.current
    if (box === null || box.closest("[inert]") !== null) return
    const active = document.activeElement
    const lost = active === null || active === document.body || !active.isConnected || active.closest("[inert]") !== null
    if (!lost) return
    const live = (selector: string) =>
      [...box.querySelectorAll<HTMLButtonElement>(selector)].find(button => button.closest("[inert]") === null && !button.disabled)
    const key = view.keyboard?.key
    const keyed = key === undefined ? undefined
      : [...box.querySelectorAll<HTMLButtonElement>("[data-key-id]")].find(button => button.dataset["keyId"] === key)
    const target = keyed ?? live(".zone-store-layer " + FOCUSED) ?? live(".zone-store-layer button") ?? live(FOCUSED) ?? live("button")
    target?.focus({ preventScroll: true })
  })

  return (
    <ZoneShell
      band={band}
      foot={clock === undefined && build === undefined ? undefined : <ZoneFoot build={build} clock={clock} />}
      hintBack={<ZoneKeycap button="B" word="Back" />}
      hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
      strips={strips}
      tabs={<ZoneBanner>Get games</ZoneBanner>}
    >
      <div className="zone-store" data-layer={layer} data-page={view.page} ref={root}>
        <div className="zone-store-head" ref={head}>
          {where === undefined ? null : (
            <span className="zone-store-where">
              <ZonePanel>
                <span className="zone-store-where-words" ref={title}>{where}</span>
              </ZonePanel>
            </span>
          )}
          {tags.map(tag => (
            <span className="zone-store-tag" key={tag.label}>
              <ZoneTag kind={tag.kind} korri={tag.korri} label={tag.label} />
            </span>
          ))}
        </div>
        <div className="zone-store-body" data-foot={typing || working !== undefined ? "" : undefined} ref={body}>
          <div className="zone-store-band" inert={layer !== "none"} ref={rowBand}>
            <div className="zone-store-scroll" data-block-exit="true" ref={list}>
              <div className="zone-store-rows">
                {view.page === "providers" ? providersBody(store, view, press, onRowFocus)
                  : view.page === "results" ? resultsBody(store, view, press, onRowFocus)
                  : entryBody(store, view, press, onRowFocus)}
              </div>
            </div>
          </div>
          {view.keyboard === undefined ? null : (
            <div className="zone-store-keys" data-block-exit="" ref={keysBox}>
              <ZoneKeyboard
                caret={view.keyboard.caret}
                draft={view.keyboard.draft}
                focusedKey={view.keyboard.key}
                layer={view.keyboard.layer}
                maxLength={ZONE_STORE_QUERY_MAX}
                onFocusKey={onKeyFocus}
                onKey={onKey}
                planned
              />
            </div>
          )}
          {/* Work under way stands at the body's foot, under the band, as the
              status layer's strip stands at the page's foot: never scrolled
              away, so CANCEL is always on screen. */}
          {working === undefined || typing ? null : (
            <div className="zone-store-busy">
              <ZoneBusyStrip
                actions={working.actions}
                detail={working.detail}
                focused={enabledFrom(working.actions, view.dialog)}
                kicker={working.kicker}
                korri
                onAction={run}
                onFocusAction={onDialogFocus}
                progress={working.progress}
              />
            </div>
          )}
          {dialog === undefined ? null : (
            <div className="zone-store-layer" data-block-exit="true">
              <div className="zone-store-dialog">
                <ZoneDialog
                  actions={dialog.actions.map(action => ({
                    id: action.id,
                    label: action.label,
                    disabled: !action.enabled,
                    destructive: action.destructive,
                    onPress: () => run(action.id),
                  }))}
                  focused={enabledFrom(dialog.actions, view.dialog)}
                  headline={dialog.headline}
                  korriHeadline={dialog.korri}
                  lines={dialog.line === undefined ? [] : [dialog.line]}
                  onFocusAction={onDialogFocus}
                />
              </div>
            </div>
          )}
        </div>
      </div>
    </ZoneShell>
  )
}

type Press = (id: string) => void
type Focus = ((id: string) => void) | undefined

/** In the head: the provider on its results, the game on its entry. Both are Korri's names. */
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

/** The entry's system (Korri's), and Zone's word for where getting it stands. */
function headTags(store: SurfaceStore, view: ZoneStoreView): readonly (ZoneTagItem & { readonly korri?: boolean })[] {
  if (view.page !== "entry" || store.details._tag === "Closed") return []
  const entry = store.details.entry
  const system = entry.system === undefined ? [] : [{ kind: "system", label: entry.system, korri: true } as const]
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

/** The open entry's work under way, when the entry is on screen. */
function workingOf(store: SurfaceStore, view: ZoneStoreView): Extract<SurfaceStoreGet, { readonly _tag: "Working" }> | undefined {
  if (view.page !== "entry" || store.details._tag !== "Ready") return undefined
  return store.details.get._tag === "Working" ? store.details.get : undefined
}

/** Korri's question or trouble now, for the dialog over the band; none when there is none. */
function dialogOf(store: SurfaceStore, view: ZoneStoreView): StoreDialog | undefined {
  if (view.keyboard !== undefined) return undefined
  switch (view.page) {
    case "providers":
      return store.providers._tag === "Problem"
        ? { headline: "Providers", korri: false, line: store.providers.message, actions: store.providers.actions }
        : undefined
    case "results": {
      const search = zoneStoreSearch(store, view.provider)
      return search._tag === "Problem" ? { headline: "Search", korri: false, line: search.message, actions: search.actions } : undefined
    }
    case "entry": {
      const details = store.details
      if (details._tag === "Problem") return { headline: details.entry.title, korri: true, line: details.message, actions: details.actions }
      if (details._tag !== "Ready") return undefined
      const get = details.get
      if (get._tag === "Confirm") return { headline: get.kicker, korri: true, line: get.detail, actions: get.actions }
      if (get._tag === "Problem") return { headline: get.kicker, korri: true, line: get.reason, actions: get.actions }
      return undefined
    }
  }
}

/** A word of Zone's on a black panel, centred: NO PROVIDERS, NOTHING FOUND. */
const word = (text: string): ReactNode => (
  <div className="zone-store-word">
    <ZonePanel>
      <ZoneCaps>{text}</ZoneCaps>
    </ZonePanel>
  </div>
)

/** Korri's words as written: Jersey 10 on a black panel at its one-line slant. */
const line = (text: string): ReactNode => (
  <p className="zone-store-line" data-option-clip="slash">{text}</p>
)

function providersBody(store: SurfaceStore, view: ZoneStoreView, press: Press, onRowFocus: Focus): ReactNode {
  const providers = store.providers
  switch (providers._tag) {
    case "Loading":
      return <ZoneBusyStrip kicker="Loading" />
    case "Problem":
      // Korri's trouble is the dialog over the band.
      return null
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
            verbatimLabel: true,
            value: "Open",
            interaction: { kind: "action", actionId: provider.id },
          }))}
          title="Providers"
        />
      )
  }
}

function resultsBody(store: SurfaceStore, view: ZoneStoreView, press: Press, onRowFocus: Focus): ReactNode {
  const search = zoneStoreSearch(store, view.provider)
  const searchRow: ZoneOptionGroupRow = {
    id: ZONE_STORE_SEARCH_ROW,
    label: "Search for",
    // What was asked is the person's words, kept in their own case.
    ...(search._tag === "Idle" ? {} : { value: search.query }),
    verbatimValue: true,
    interaction: { kind: "text" },
  }
  const found = search._tag === "Ready" && search.entries.length > 0
  return (
    <>
      <ZoneOptionGroup
        focusedId={view.focused}
        followed={found}
        onFocus={onRowFocus}
        onPress={press}
        rows={[searchRow]}
        title="Search"
      />
      {(() => {
        switch (search._tag) {
          case "Idle":
          case "Problem":
            return null
          case "Searching":
            return <ZoneBusyStrip kicker="Searching" />
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
                  verbatimLabel: true,
                  // The system as the provider says it. When it does not say, the
                  // value is absent and Zone writes nothing in its place.
                  ...(entry.system === undefined ? {} : { value: entry.system }),
                  verbatimValue: true,
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

function entryBody(store: SurfaceStore, view: ZoneStoreView, press: Press, onRowFocus: Focus): ReactNode {
  const details = store.details
  if (details._tag === "Closed") return <ZoneBusyStrip kicker="Loading" />
  const get = details._tag === "Ready" ? details.get : undefined
  return (
    <>
      <div className="zone-store-entry">
        <span className="zone-store-well">
          <ZoneCoverWell title={details.entry.title} />
        </span>
        <div className="zone-store-entry-text">
          {details._tag === "Ready" && details.description !== undefined ? line(details.description) : null}
          {get === undefined ? null : getting(get, view, press, onRowFocus)}
        </div>
      </div>
      {details._tag === "Loading" ? <ZoneBusyStrip kicker="Loading" /> : null}
    </>
  )
}

/** Where getting the entry stands, with Korri's own actions. Its question and trouble are the dialog. */
function getting(get: SurfaceStoreGet, view: ZoneStoreView, press: Press, onRowFocus: Focus): ReactNode {
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
          {get.note === undefined ? null : line(get.note)}
          {row(get.actions.map(action => ({
            id: zoneStoreActionRow(action.id),
            label: action.label,
            ...(action.enabled ? {} : { disabled: true }),
          })))}
        </>
      )
    case "Unavailable":
      return line(get.reason)
    case "Confirm":
    case "Problem":
    case "Working":
      // The question and trouble are the dialog; the work, the body's foot.
      return null
    case "InLibrary":
      return (
        <>
          {get.detail === undefined ? null : line(get.detail)}
          {get.gameId === undefined ? null : row([{ id: zoneStorePlayRow(get.gameId), label: "Play" }])}
        </>
      )
  }
}
