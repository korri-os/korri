import "./ZoneGames.css"
import type { SurfaceAction, SurfaceNetworkConnection, SurfaceSettingItem } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef, type FocusEvent } from "react"
import { ZoneButton } from "../ui/atoms/ZoneButton"
import { ZoneCaps } from "../ui/atoms/ZoneCaps"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZonePanel } from "../ui/atoms/ZonePanel"
import { ZoneActionRow, type ZoneAction } from "../ui/molecules/ZoneActionRow"
import { ZoneBusyStrip } from "../ui/molecules/ZoneBusyStrip"
import { ZoneCardBand, type ZoneBandCard } from "../ui/molecules/ZoneCardBand"
import { ZoneDialog } from "../ui/molecules/ZoneDialog"
import { ZoneFoot } from "../ui/molecules/ZoneFoot"
import { ZoneKeyboard, type ZoneKey, type ZoneKeyboardLayer } from "../ui/molecules/ZoneKeyboard"
import { ZoneOptionGroup } from "../ui/molecules/ZoneOptionGroup"
import { ZoneTabStrip, type ZoneTabItem } from "../ui/molecules/ZoneTabStrip"
import { ZoneTitleCard } from "../ui/molecules/ZoneTitleCard"
import { ZoneShell } from "../ui/templates/ZoneShell"

export type ZoneGamesTab = "games" | "options" | "savedata"
export type ZoneGamesZone = "tabs" | "find" | "cards" | "actions"

/** What Korri says about the catalog, without the games (they come as cards). */
export type ZoneGamesCatalog =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Error"; readonly message: string }
  | { readonly _tag: "Empty" }
  | { readonly _tag: "Ready" }

/** One button in the action row, as the state's launch rules give it. */
export interface ZoneGamesLaunch {
  readonly id: string
  readonly verb: "play" | "continue"
  readonly place?: { readonly here: true } | { readonly here: false; readonly label: string }
  readonly locationId?: string
}

/** Korri's session facts for the focused game; each only when published. */
export interface ZoneGamesFacts {
  readonly lastPlayedAt?: number
  readonly playCount?: number
  readonly playtime?: { readonly hours: number; readonly minutes: number }
}

export interface ZoneGamesFind {
  /** The filter. Empty: no filter and no FIND tag. */
  readonly draft: string
  readonly caret: number
  readonly layer: ZoneKeyboardLayer
  readonly key: string
}

export interface ZoneGamesOpts {
  readonly gameTitle: string
  /** Korri's actions for the game, verbatim. */
  readonly actions: readonly SurfaceAction[]
  readonly row: number
  /** The destructive action awaiting NO / YES. */
  readonly asking?: SurfaceAction
  /** NO is 0, YES is 1. */
  readonly answer: number
}

/** Every fact the page reports. The binding hands these to `update`. */
export type ZoneGamesEvent =
  | { readonly _tag: "FocusedTab"; readonly tab: ZoneGamesTab }
  | { readonly _tag: "ChoseTab"; readonly tab: ZoneGamesTab }
  | { readonly _tag: "FocusedFind" }
  | { readonly _tag: "ChoseFind" }
  | { readonly _tag: "FocusedCard"; readonly index: number }
  | { readonly _tag: "ChoseCard"; readonly index: number }
  | { readonly _tag: "FocusedAction"; readonly index: number }
  | { readonly _tag: "ChoseLaunch"; readonly gameId: string; readonly locationId?: string }
  | { readonly _tag: "FocusedKey"; readonly id: string }
  | { readonly _tag: "PressedKey"; readonly key: ZoneKey }
  | { readonly _tag: "FocusedGameOpt"; readonly index: number }
  | {
      readonly _tag: "ChoseGameAction"
      readonly gameId: string
      readonly actionId: string
      readonly enabled: boolean
      readonly destructive: boolean
    }
  | { readonly _tag: "FocusedAnswer"; readonly index: number }
  | { readonly _tag: "AnsweredGameAction"; readonly yes: boolean }
  | { readonly _tag: "OpenedRunWith"; readonly gameId: string }
  | { readonly _tag: "ChoseReload" }

export interface ZoneGamesProps {
  /** The plan's two answers: height picks the strips, width the band. */
  readonly strips: "tall" | "short"
  readonly band: "wide" | "narrow"
  /** The plan's answer for the facts line: its own row, or in the action strip. */
  readonly factsAt: "row" | "inline"
  readonly tab: ZoneGamesTab
  readonly tabCursor: ZoneGamesTab
  readonly zone: ZoneGamesZone
  readonly catalog: ZoneGamesCatalog
  /** The band: the catalog through the find draft, sections marked. */
  readonly cards: readonly ZoneBandCard[]
  readonly focusedCard: number
  readonly facts?: ZoneGamesFacts
  readonly launches: readonly ZoneGamesLaunch[]
  readonly focusedAction: number
  readonly find: ZoneGamesFind
  /** The keyboard is up over the action row. */
  readonly finding: boolean
  /** GAME OPTS. is open over the band. */
  readonly opts?: ZoneGamesOpts
  /** The idle title card: the game it shows now. */
  readonly idle?: { readonly title: string; readonly act?: string }
  readonly clock?: string
  readonly device?: string
  readonly connection?: SurfaceNetworkConnection
  readonly build?: string
  readonly onEvent: (event: ZoneGamesEvent) => void
}

const TABS: readonly (ZoneTabItem & { readonly id: ZoneGamesTab })[] = [
  { id: "games", label: "Games" },
  { id: "options", label: "Options", sub: "Settings + Wi-Fi" },
  { id: "savedata", label: "Save data", sub: "Identity" },
]

/** The row GAME OPTS. adds after Korri's actions: the OPTIONS page's runner choice. */
const RUN_WITH = "zone-run-with"

/** The three stripe-well cards the band shows while Korri loads: no titles, no tags. */
const LOADING_CARDS: readonly ZoneBandCard[] = ["", " ", "  "].map((title, index) => ({
  id: `loading-${index}`,
  title,
  places: [],
}))

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]

/** A day in the person's own time zone: 19 Oct 2026. */
const day = (at: number): string => {
  const date = new Date(at)
  return `${date.getDate()} ${MONTHS[date.getMonth()] ?? ""} ${date.getFullYear()}`
}

const hoursAndMinutes = ({ hours, minutes }: { readonly hours: number; readonly minutes: number }): string =>
  hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`

/** PLAY, CONTINUE, PLAY HERE, PLAY ON ZAO. */
export const launchLabel = (launch: ZoneGamesLaunch): string => {
  const verb = launch.verb === "continue" ? "Continue" : "Play"
  if (launch.place === undefined) return verb
  return launch.place.here ? `${verb} here` : `${verb} on ${launch.place.label}`
}

/** A keyboard key's id from the button that drew it: its glyph, or the control it names. */
const KEY_CONTROLS: Readonly<Record<string, string>> = {
  Space: "space",
  Delete: "delete",
  Done: "done",
  abc: "layer:letters",
  ABC: "layer:shift",
  "#?!": "layer:symbols",
  "://": "layer:uri",
}
const keyIdOf = (text: string): string => KEY_CONTROLS[text] ?? text

/** The buttons in a region, in order: the page's handle on DOM focus. */
const buttonsIn = (region: Element | null): HTMLButtonElement[] =>
  region === null ? [] : [...region.querySelectorAll("button")]

const indexIn = (region: Element, target: EventTarget): number =>
  buttonsIn(region).findIndex(button => button === target || button.contains(target as Node))

/**
 * The GAMES page: Zone's home and the zone select a person sees on power-on.
 *
 * The shell's top strip holds the B and A hints, the three tabs and the
 * FIND key at their right end. The body is the card band with the focused
 * game centred, a line of Korri's session facts under it, and the action
 * row: PLAY, CONTINUE, or one button per place when there is a choice. The
 * foot reads the clock, the device, the network and the build.
 *
 * Over the page, one at a time: GAME OPTS. over the band, the keyboard in
 * place of the facts and actions while finding, the catalog's error dialog,
 * and the idle title card over everything.
 *
 * A shape: every value is a prop and every fact goes out through `onEvent`.
 * The one mechanic it keeps is DOM focus. When the props say the cursor is
 * somewhere DOM focus is not (after menu, back, or a layer opening), the
 * page moves focus there; when the host moves focus, the page reports it.
 */
export function ZoneGames(props: ZoneGamesProps) {
  const {
    strips, band, factsAt, tab, tabCursor, zone, catalog, cards, focusedCard, facts, launches, focusedAction,
    find, finding, opts, idle, clock, device, connection, build, onEvent,
  } = props
  const root = useRef<HTMLDivElement | null>(null)
  // True while the page itself moves DOM focus to where the props say it is:
  // that move is not news, and reporting it would wake idle.
  const syncing = useRef(false)
  const report = (event: ZoneGamesEvent) => {
    if (!syncing.current) onEvent(event)
  }
  const games = tab === "games"
  const game = games ? cards[focusedCard] : undefined
  const ready = catalog._tag === "Ready" && cards.length > 0
  const choice = launches.length > 1
  const failed = games && catalog._tag === "Error"
  const layer = opts !== undefined ? "opts" : finding ? "keyboard" : failed ? "error" : "none"

  // Where DOM focus belongs, as a region and an index in it.
  const target: readonly [string, number | string] | undefined =
    opts !== undefined ? (opts.asking !== undefined ? ["answer", opts.answer] : ["opts", opts.row])
    : finding ? ["keys", find.key]
    : zone === "tabs" || !games ? ["tabs", Math.max(0, TABS.findIndex(item => item.id === tabCursor))]
    : zone === "find" ? ["find", 0]
    : failed ? ["error", 0]
    : zone === "actions" && ready ? ["actions", focusedAction]
    : ready ? ["band", focusedCard]
    // A find that matches nothing: the cursor waits on FIND, to change it.
    : catalog._tag === "Ready" ? ["find", 0]
    : catalog._tag === "Empty" ? ["tabs", 0]
    : undefined
  const targetKey = target === undefined ? "" : `${target[0]}:${target[1]}:${idle === undefined ? "awake" : "idle"}`

  useLayoutEffect(() => {
    if (target === undefined || root.current === null) return
    const [region, at] = target
    const buttons = buttonsIn(root.current.querySelector(`[data-games-region="${region}"]`))
    const button = typeof at === "number"
      ? buttons[Math.min(at, buttons.length - 1)]
      : buttons.find(candidate => keyIdOf(candidate.textContent ?? "") === at) ?? buttons[0]
    if (button === undefined || document.activeElement === button) return
    syncing.current = true
    button.focus({ preventScroll: true })
    syncing.current = false
    // targetKey names everything target says; the effect follows it alone.
  }, [targetKey])

  const optRows: readonly SurfaceSettingItem[] = opts === undefined ? [] : [
    ...opts.actions.map((action): SurfaceSettingItem => action.enabled
      ? {
        id: action.id,
        label: action.label,
        value: "Run",
        description: action.description,
        interaction: { kind: "action", actionId: action.id, destructive: action.destructive },
      }
      // Korri says why it cannot: its description stands where the verb would.
      : { id: action.id, label: action.label, value: action.description }),
    { id: RUN_WITH, label: "Run with", value: "Choose", interaction: { kind: "action", actionId: RUN_WITH } },
  ]

  const actions: readonly ZoneAction[] = launches.map(launch => ({ id: launch.id, label: launchLabel(launch) }))
  const focusedLaunch = zone === "actions" ? launches[focusedAction]?.id : undefined
  // One place: its button is the default and stays lit. A choice: nothing is
  // lit until the cursor is on one.
  const lit = choice ? focusedLaunch : launches[0]?.id

  const onKeysFocus = (event: FocusEvent<HTMLDivElement>) => {
    const button = (event.target as Element).closest("button")
    if (button !== null) report({ _tag: "FocusedKey", id: keyIdOf(button.textContent ?? "") })
  }
  const onAnswerFocus = (event: FocusEvent<HTMLDivElement>) => {
    const index = indexIn(event.currentTarget, event.target)
    if (index >= 0) report({ _tag: "FocusedAnswer", index })
  }

  const factLines = facts === undefined ? [] : [
    ...(facts.lastPlayedAt === undefined ? [] : [["Last played", day(facts.lastPlayedAt)] as const]),
    ...(facts.playCount === undefined ? [] : [["Plays", String(facts.playCount)] as const]),
    ...(facts.playtime === undefined ? [] : [["Time", hoursAndMinutes(facts.playtime)] as const]),
  ]

  const tabsSlot = (
    <div className="zone-games-tabs" data-games-region="tabs">
      <ZoneTabStrip
        active={tab}
        focused={target?.[0] === "tabs" ? TABS[Number(target[1])]?.id : undefined}
        onFocus={id => report({ _tag: "FocusedTab", tab: id as ZoneGamesTab })}
        onSelect={id => onEvent({ _tag: "ChoseTab", tab: id as ZoneGamesTab })}
        tabs={TABS}
      />
    </div>
  )

  return (
    <div className="zone-games" data-layer={layer} data-strips={strips} ref={root}>
      <ZoneShell
        band={band}
        foot={<ZoneFoot build={build} clock={clock} connection={connection} device={device} />}
        hintBack={<ZoneKeycap button="B" word="Back" />}
        hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
        strips={strips}
        tabs={tabsSlot}
      >
        {games ? (
          <div className="zone-games-body" data-facts={factsAt}>
            <div className="zone-games-band" data-games-region="band" inert={layer !== "none"}>
              {catalog._tag === "Loading" ? (
                <div className="zone-games-loading" inert>
                  <ZoneCardBand cards={LOADING_CARDS} focusedIndex={1} visible={3} />
                </div>
              ) : (
                <ZoneCardBand
                  cards={catalog._tag === "Ready" ? cards : []}
                  focusedIndex={focusedCard}
                  onFocus={index => report({ _tag: "FocusedCard", index })}
                  visible={band === "wide" ? 5 : 3}
                />
              )}
              {find.draft === "" ? null : (
                <div className="zone-games-find-tag">
                  <ZonePanel>
                    <ZoneCaps>{`Find: ${find.draft}`}</ZoneCaps>
                  </ZonePanel>
                </div>
              )}
            </div>
            {catalog._tag !== "Ready" ? null : (
              <div className="zone-games-find" data-games-region="find" inert={layer !== "none"}>
                <ZoneButton
                  focused={target?.[0] === "find"}
                  label="Find"
                  lit={find.draft !== ""}
                  onFocus={() => report({ _tag: "FocusedFind" })}
                  onPress={() => onEvent({ _tag: "ChoseFind" })}
                />
              </div>
            )}
            {finding ? (
              <div className="zone-games-keys" data-block-exit="" data-games-region="keys" onFocus={onKeysFocus}>
                <ZoneKeyboard
                  caret={find.caret}
                  draft={find.draft}
                  focusedKey={find.key}
                  layer={find.layer}
                  onKey={key => onEvent({ _tag: "PressedKey", key })}
                />
              </div>
            ) : (
              <>
                <div className="zone-games-facts" data-empty={factLines.length === 0 || undefined} inert={layer !== "none"}>
                  {factLines.length === 0 || game === undefined ? (
                    // The line keeps its height when Korri has no facts, so the band
                    // does not jump as the cursor passes a game never played.
                    <ZonePanel edge={false}>
                      <ZoneCaps>{"\u00a0"}</ZoneCaps>
                    </ZonePanel>
                  ) : (
                    <ZonePanel edge={false}>
                      <span className="zone-games-fact-run">
                        {factLines.map(([label, value]) => (
                          <span className="zone-games-fact" key={label}>
                            <span className="zone-games-fact-label">{label}</span>
                            <ZoneCaps>{value}</ZoneCaps>
                          </span>
                        ))}
                      </span>
                    </ZonePanel>
                  )}
                </div>
                <div className="zone-games-actions" data-games-region="actions" inert={layer !== "none"}>
                  {catalog._tag === "Loading" ? (
                    <div className="zone-games-busy">
                      <ZoneBusyStrip kicker="Loading" />
                    </div>
                  ) : null}
                  {ready && game !== undefined ? (
                    <ZoneActionRow
                      actions={actions}
                      focused={focusedLaunch}
                      lit={lit}
                      onFocus={id => report({ _tag: "FocusedAction", index: launches.findIndex(launch => launch.id === id) })}
                      onPress={id => {
                        const launch = launches.find(candidate => candidate.id === id)
                        if (launch !== undefined) onEvent({ _tag: "ChoseLaunch", gameId: game.id, locationId: launch.locationId })
                      }}
                    />
                  ) : null}
                </div>
              </>
            )}
            {opts !== undefined && game !== undefined ? (
              <div className="zone-games-opts" data-block-exit="">
                <div className="zone-games-opts-head">
                  <ZonePanel>
                    <ZoneCaps>Game opts.</ZoneCaps>
                  </ZonePanel>
                </div>
                <div
                  className="zone-games-opts-rows"
                  data-games-region="opts"
                  inert={opts.asking !== undefined}
                >
                  <ZoneOptionGroup
                    focusedId={optRows[opts.row]?.id}
                    onFocus={id => report({ _tag: "FocusedGameOpt", index: optRows.findIndex(row => row.id === id) })}
                    onPress={id => {
                      if (id === RUN_WITH) return onEvent({ _tag: "OpenedRunWith", gameId: game.id })
                      const action = opts.actions.find(candidate => candidate.id === id)
                      if (action === undefined) return
                      onEvent({
                        _tag: "ChoseGameAction",
                        gameId: game.id,
                        actionId: action.id,
                        enabled: action.enabled,
                        destructive: action.destructive === true,
                      })
                    }}
                    rows={optRows}
                    title={opts.gameTitle}
                  />
                </div>
                {opts.asking === undefined ? null : (
                  <div className="zone-games-ask" data-games-region="answer" onFocus={onAnswerFocus}>
                    <div className="zone-games-dialog">
                      <ZoneDialog
                        actions={[
                          { label: "No", onPress: () => onEvent({ _tag: "AnsweredGameAction", yes: false }) },
                          { label: "Yes", onPress: () => onEvent({ _tag: "AnsweredGameAction", yes: true }) },
                        ]}
                        focused={opts.answer}
                        headline={opts.asking.label}
                        lines={opts.asking.description === undefined ? [] : [opts.asking.description]}
                        title={opts.gameTitle}
                      />
                    </div>
                  </div>
                )}
              </div>
            ) : null}
            {failed ? (
              <div className="zone-games-error" data-block-exit="" data-games-region="error">
                <div className="zone-games-dialog">
                  <ZoneDialog
                    actions={[{ label: "Reload", onPress: () => onEvent({ _tag: "ChoseReload" }) }]}
                    focused={0}
                    headline={catalog.message}
                    lines={[]}
                  />
                </div>
              </div>
            ) : null}
          </div>
        ) : (
          <div className="zone-games-elsewhere">
            <ZonePanel>
              <ZoneCaps>{TABS.find(item => item.id === tab)?.label ?? ""}</ZoneCaps>
            </ZonePanel>
          </div>
        )}
      </ZoneShell>
      {idle === undefined ? null : (
        <div aria-live="off" className="zone-games-idle">
          <ZoneTitleCard act={idle.act} title={idle.title} />
        </div>
      )}
    </div>
  )
}
