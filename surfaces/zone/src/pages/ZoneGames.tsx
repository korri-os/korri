import "./ZoneGames.css"
import type { SurfaceAction, SurfaceNetworkConnection, SurfaceSettingItem } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef } from "react"
import { formatZoneBytes } from "../plan/bytes"
import { ZONE_SHEET_MS, zoneSheetFrame } from "../plan/motion"
import { useZoneFindPlan } from "../use-zone-find-plan"
import { useZoneMoment } from "../use-zone-moment"
import { ZoneButton } from "../ui/atoms/ZoneButton"
import { ZoneCaps } from "../ui/atoms/ZoneCaps"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZonePanel } from "../ui/atoms/ZonePanel"
import { ZoneTag } from "../ui/atoms/ZoneTag"
import { ZoneActionRow, type ZoneAction } from "../ui/molecules/ZoneActionRow"
import { ZoneBusyStrip } from "../ui/molecules/ZoneBusyStrip"
import { cardTags } from "../ui/molecules/ZoneCard"
import { ZoneCardBand, type ZoneBandCard } from "../ui/molecules/ZoneCardBand"
import { ZoneDialog } from "../ui/molecules/ZoneDialog"
import { ZoneFoot } from "../ui/molecules/ZoneFoot"
import { ZoneKeyboard, type ZoneKey, type ZoneKeyboardLayer } from "../ui/molecules/ZoneKeyboard"
import type { ZoneOptionGroupRow } from "../ui/molecules/ZoneOptionGroup"
import { ZoneOptionRow } from "../ui/molecules/ZoneOptionRow"
import { ZoneTabStrip, type ZoneTabItem } from "../ui/molecules/ZoneTabStrip"
import { ZoneAttract } from "../ui/organisms/ZoneAttract"
import { ZoneGround } from "../ui/organisms/ZoneGround"
import { ZoneTally } from "../ui/organisms/ZoneTally"
import { ZoneShell } from "../ui/templates/ZoneShell"

export type ZoneGamesTab = "games" | "options" | "savedata"
export type ZoneGamesZone = "tabs" | "find" | "get" | "cards" | "actions"

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
  /** Korri's `download`: the first launch fetches something; `bytes` when known. */
  readonly download?: { readonly bytes?: number }
  /** The launch profile PLAY uses, in Korri's words; absent when none or unread. */
  readonly profile?: string
}

export interface ZoneGamesFind {
  /** The filter. Empty: no filter and no FIND tag. */
  readonly draft: string
  readonly caret: number
  readonly layer: ZoneKeyboardLayer
  readonly key: string
  /** The most characters the draft holds. */
  readonly max?: number
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
  | { readonly _tag: "FocusedGetGames" }
  | { readonly _tag: "ChoseGetGames" }
  | { readonly _tag: "FocusedCard"; readonly index: number }
  | { readonly _tag: "ChoseCard"; readonly index: number }
  | { readonly _tag: "FocusedAction"; readonly index: number }
  | { readonly _tag: "ChoseLaunch"; readonly gameId: string; readonly locationId?: string }
  | { readonly _tag: "FocusedKey"; readonly id: string }
  | { readonly _tag: "PressedKey"; readonly key: ZoneKey }
  | { readonly _tag: "ChoseGameOpts" }
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
  | { readonly _tag: "ChoseReload" }
  | { readonly _tag: "PressedTally" }

/** The idle attract's game this turn: its title card, then its stage. */
export interface ZoneGamesIdle {
  /** Names the turn: a new one starts the attract's clock again. */
  readonly key: string
  readonly title: string
  readonly system?: string
  /** The session that would start next: Korri's playCount + 1. */
  readonly act: number
  /** Korri's cover art. */
  readonly art?: string
  readonly plays?: number
}

/** The results tally after a session whose count rose by exactly one. */
export interface ZoneGamesTally {
  readonly gameId: string
  readonly title: string
  readonly seconds?: number
  readonly plays: number
  readonly total?: number
  readonly counted: boolean
  readonly leaving: boolean
}

export interface ZoneGamesProps {
  /** The plan's two answers: height picks the strips, width the band. */
  readonly strips: "tall" | "short"
  readonly band: "wide" | "narrow"
  /** The plan's answer for the facts line: its own row, or in the action strip. */
  readonly factsAt: "row" | "inline"
  /** The plan's answer for the find keyboard: in the body, or over the screen below the tabs. */
  readonly keysAt: "body" | "screen"
  readonly tab: ZoneGamesTab
  readonly tabCursor: ZoneGamesTab
  readonly zone: ZoneGamesZone
  readonly catalog: ZoneGamesCatalog
  /** Korri can get games from a provider here: the GET GAMES key shows. */
  readonly getGames: boolean
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
  /** The idle attract: the game whose turn it is. */
  readonly idle?: ZoneGamesIdle
  /**
   * The launch title card (the status layer's stage, over this page) holds
   * the page: Korri's status has named the launch and the card is not
   * leaving. Before that the card is only Zone's announcement, and a move
   * under it lets go of it.
   */
  readonly launchHeld?: boolean
  /** The results tally over the page. */
  readonly tally?: ZoneGamesTally
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

/** The three cards the band shows while Korri loads: static in their wells, no titles, no tags. */
const LOADING_CARDS: readonly ZoneBandCard[] = ["", " ", "  "].map((title, index) => ({
  id: `loading-${index}`,
  title,
  places: [],
  noise: true,
}))

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]

/** A day in the person's own time zone: 19 Oct 2026. */
const day = (at: number): string => {
  const date = new Date(at)
  return `${date.getDate()} ${MONTHS[date.getMonth()] ?? ""} ${date.getFullYear()}`
}

const hoursAndMinutes = ({ hours, minutes }: { readonly hours: number; readonly minutes: number }): string =>
  hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`

/** TIME ATTACK's empty record: the dashes a fact Korri has not recorded shows. */
const NO_TIME = "--'--\"--"
const NO_VALUE = "--"

/** PLAY, CONTINUE, PLAY HERE, PLAY ON ZAO. */
export const launchLabel = (launch: ZoneGamesLaunch): string => {
  const verb = launch.verb === "continue" ? "Continue" : "Play"
  if (launch.place === undefined) return verb
  return launch.place.here ? `${verb} here` : `${verb} on ${launch.place.label}`
}

/** The OPTS. button's id in the action row; no launch choice is named so. */
const OPTS_ID = "game-opts"

/** The buttons in a region, in order: the page's handle on DOM focus. */
const buttonsIn = (region: Element | null): HTMLButtonElement[] =>
  region === null ? [] : [...region.querySelectorAll("button")]

/**
 * The GAMES page: Zone's home and the zone select a person sees on power-on.
 *
 * The shell's top strip holds the B and A hints and the three tabs. The
 * body's top corners hold two keys: FIND on the right, under the A hint,
 * and GET GAMES on the left, under the B hint, whenever Korri can get
 * games (an empty library too, where it is the one thing to do). The body is the card band with the focused
 * game centred, a line of Korri's session facts under it, and the action
 * row: PLAY, CONTINUE, or one button per place when there is a choice,
 * then OPTS., which opens GAME OPTS. as the host's `options` does. The
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
    strips, band, factsAt, keysAt, tab, tabCursor, zone, catalog, getGames, cards, focusedCard, facts, launches, focusedAction,
    find, finding, opts, idle, launchHeld = false, tally, clock, device, connection, build, onEvent,
  } = props
  const root = useRef<HTMLDivElement | null>(null)
  const body = useRef<HTMLDivElement | null>(null)
  const keysBox = useRef<HTMLDivElement | null>(null)
  // GAME OPTS. slides in over the band from the right (M2's incoming rows).
  const sheet = useRef<HTMLDivElement | null>(null)
  useZoneMoment(sheet, {
    key: opts === undefined ? undefined : "open",
    duration: ZONE_SHEET_MS,
    frame: (t, size) => ({ "--zone-games-sheet-x": zoneSheetFrame(t, size.wArt, size.drawing.motion) }),
  })
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
  // The keys and the band share the box under the tabs by plan, so no key
  // is ever cut and the keys never scroll.
  useZoneFindPlan(root, body, keysBox, finding && games ? keysAt : undefined)
  // The tally covers the page, makes it inert and holds the cursor itself.
  // The launch card (drawn over the page by the status layer) does too
  // once Korri has named the launch.
  // Idle covers nothing: a key wakes it.
  const holding = tally !== undefined && !tally.leaving
  const launching = launchHeld

  // Where DOM focus belongs, as a region and an index in it.
  const target: readonly [string, number | string] | undefined =
    holding ? ["tally", 0]
    : launching ? undefined
    : opts !== undefined ? (opts.asking !== undefined ? ["answer", opts.answer] : ["opts", opts.row])
    : finding ? ["keys", find.key]
    : zone === "tabs" || !games ? ["tabs", Math.max(0, TABS.findIndex(item => item.id === tabCursor))]
    : zone === "find" ? ["find", 0]
    : zone === "get" && getGames ? ["get", 0]
    : failed ? ["error", 0]
    : zone === "actions" && ready ? ["actions", focusedAction]
    : ready ? ["band", focusedCard]
    // A find that matches nothing: the cursor waits on FIND, to change it.
    : catalog._tag === "Ready" ? ["find", 0]
    : catalog._tag === "Empty" ? (getGames ? ["get", 0] : ["tabs", 0])
    : undefined
  const targetKey = target === undefined ? "" : `${target[0]}:${target[1]}:${idle === undefined ? "awake" : "idle"}:${launching}`

  useLayoutEffect(() => {
    if (target === undefined || root.current === null) return
    const [region, at] = target
    const buttons = buttonsIn(root.current.querySelector(`[data-games-region="${region}"]`))
    const button = typeof at === "number"
      ? buttons[Math.min(at, buttons.length - 1)]
      : buttons.find(candidate => candidate.dataset["keyId"] === at) ?? buttons[0]
    if (button === undefined || document.activeElement === button) return
    syncing.current = true
    button.focus({ preventScroll: true })
    syncing.current = false
    // targetKey names everything target says; the effect follows it alone.
  }, [targetKey])

  const optRows: readonly ZoneOptionGroupRow[] = opts === undefined ? [] : [
    // A disabled action keeps its verb, dimmed; Korri's description under
    // it says why, and A does nothing.
    ...opts.actions.map((action): ZoneOptionGroupRow => ({
      id: action.id,
      label: action.label,
      value: "Run",
      description: action.description,
      interaction: { kind: "action", actionId: action.id, destructive: action.destructive },
      ...(action.enabled ? {} : { disabled: true }),
    })),
  ]

  // The play buttons, then OPTS.: GAME OPTS. by d-pad and A, as `options` opens it.
  const actions: readonly ZoneAction[] = [
    ...launches.map(launch => ({ id: launch.id, label: launchLabel(launch) })),
    { id: OPTS_ID, label: "Opts." },
  ]
  const focusedLaunch = zone === "actions" ? actions[focusedAction]?.id : undefined
  // One place: its button is the default and stays lit. A choice: nothing is
  // lit until the cursor is on one.
  const lit = choice ? focusedLaunch : launches[0]?.id

  // The facts line as a HUD: Korri's facts, or TIME ATTACK's dashes for a
  // fact it has not recorded. DOWNLOAD only when Korri says so.
  const factLines: readonly (readonly [string, string | undefined, boolean])[] = facts === undefined || game === undefined ? [] : [
    ["Last played", facts.lastPlayedAt === undefined ? NO_VALUE : day(facts.lastPlayedAt), facts.lastPlayedAt !== undefined],
    ["Plays", facts.playCount === undefined ? NO_VALUE : String(facts.playCount), facts.playCount !== undefined],
    ["Time", facts.playtime === undefined ? NO_TIME : hoursAndMinutes(facts.playtime), facts.playtime !== undefined],
    // DOWNLOAD and its size, or the word alone when Korri does not know it.
    ...(facts.download === undefined ? []
      : [["Download", facts.download.bytes === undefined ? undefined : formatZoneBytes(facts.download.bytes), true] as const]),
    // PROFILE: the launch profile PLAY uses, when Korri names one.
    ...(facts.profile === undefined ? [] : [["Profile", facts.profile, true] as const]),
  ]
  const recorded = factLines.some(([, , known]) => known)

  // The focused card's tags its own row could not keep stand at the facts
  // line's start, so the whole set is on screen. DOWNLOAD is a fact already.
  const spillTags = game === undefined ? [] : cardTags(game)
  const spill = useRef<HTMLSpanElement | null>(null)
  const spillFrom = (shown: number) => {
    // Next task, not inside the band's resize notification: the facts line
    // is observed too, and changing it there would be a notification lost.
    setTimeout(() => {
      const tags = spill.current?.querySelectorAll<HTMLElement>(".zone-tag") ?? []
      tags.forEach((tag, index) => {
        const kept = index < shown || spillTags[index]?.kind === "download"
        tag.toggleAttribute("data-tag-hidden", kept)
      })
      spill.current?.toggleAttribute("data-empty", [...tags].every(tag => tag.hasAttribute("data-tag-hidden")))
    }, 0)
  }

  // A focused card with no room for its title (a word wider than the card,
  // lines taller than its room) draws none; the facts line shows it whole.
  const titleLine = useRef<HTMLSpanElement | null>(null)
  const titleFrom = (shown: boolean) => {
    setTimeout(() => titleLine.current?.toggleAttribute("data-hidden", shown), 0)
  }

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

  const keyboard = (
    <div className="zone-games-keys" data-block-exit="" data-games-region="keys" data-keys={keysAt} ref={keysBox}>
      <ZoneKeyboard
        caret={find.caret}
        draft={find.draft}
        focusedKey={find.key}
        layer={find.layer}
        maxLength={find.max}
        onFocusKey={id => report({ _tag: "FocusedKey", id })}
        onKey={key => onEvent({ _tag: "PressedKey", key })}
        planned
      />
    </div>
  )

  return (
    <div className="zone-games" data-layer={layer} data-strips={strips} ref={root}>
      <div className="zone-games-page" inert={holding || launching}>
      <ZoneShell
        band={band}
        foot={<ZoneFoot build={build} clock={clock} connection={connection} device={device} />}
        hintBack={<ZoneKeycap button="B" word="Back" />}
        hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
        page={tab}
        strips={strips}
        tabs={tabsSlot}
      >
        {games ? (
          <div
            className="zone-games-body"
            // GET GAMES' key is as wide as the room beside a narrow band's
            // focused card: there the band starts under the corner keys.
            data-corner={getGames && band === "narrow" && !finding ? "row" : undefined}
            data-facts={factsAt}
            ref={body}
          >
            <div className="zone-games-band" data-games-region="band" inert={layer !== "none"}>
              {catalog._tag === "Loading" ? (
                <div className="zone-games-loading" inert>
                  <ZoneCardBand cards={LOADING_CARDS} focusedIndex={1} visible={3} />
                </div>
              ) : catalog._tag === "Error" ? null : (
                <ZoneCardBand
                  cards={catalog._tag === "Ready" ? cards : []}
                  focusedIndex={focusedCard}
                  onChoose={index => onEvent({ _tag: "ChoseCard", index })}
                  onFocus={index => report({ _tag: "FocusedCard", index })}
                  onFocusedTagsShown={spillFrom}
                  onFocusedTitleShown={titleFrom}
                  visible={band === "wide" ? 5 : 3}
                />
              )}
            </div>
            {/* The body's top left: GET GAMES, then the kept filter's tag
                under it, over the first card's corner. */}
            {!getGames && find.draft === "" ? null : (
              <div className="zone-games-corner" inert={layer !== "none"}>
                {getGames ? (
                  <div className="zone-games-get" data-games-region="get">
                    <ZoneButton
                      focused={target?.[0] === "get"}
                      label="Get games"
                      onFocus={() => report({ _tag: "FocusedGetGames" })}
                      onPress={() => onEvent({ _tag: "ChoseGetGames" })}
                    />
                  </div>
                ) : null}
                {find.draft === "" ? null : (
                  <div className="zone-games-find-tag">
                    <ZonePanel>
                      <span className="zone-games-find-read">{`Find: ${find.draft}`}</span>
                      <span aria-hidden="true" className="zone-games-find-words">
                        <ZoneCaps ink="hud">Find:</ZoneCaps>
                        <ZoneCaps fact>{find.draft}</ZoneCaps>
                      </span>
                    </ZonePanel>
                  </div>
                )}
              </div>
            )}
            {catalog._tag !== "Ready" ? null : (
              <div className="zone-games-find" data-games-region="find" inert={layer !== "none"}>
                <ZoneButton
                  focused={target?.[0] === "find"}
                  label="Find"
                  onFocus={() => report({ _tag: "FocusedFind" })}
                  onPress={() => onEvent({ _tag: "ChoseFind" })}
                />
              </div>
            )}
            {finding && keysAt === "body" ? keyboard : finding ? null : (
              <>
                <div className="zone-games-factline" data-none={factLines.length === 0 || undefined} inert={layer !== "none"}>
                  <span className="zone-games-fact-panel">
                    <span className="zone-games-fact-run">
                      {game === undefined ? null : (
                        // Generated from the attribute: no second copy of the title in the page's text.
                        <span className="zone-games-fact-title" data-hidden="" data-title={game.title} ref={titleLine} />
                      )}
                      <span className="zone-games-spill" data-empty="" ref={spill}>
                        {spillTags.map(tag => (
                          <ZoneTag key={`${tag.kind}:${tag.label}`} kind={tag.kind} korri={tag.korri} label={tag.label} />
                        ))}
                      </span>
                      <span className="zone-games-facts" data-empty={!recorded || undefined}>
                        {factLines.length === 0 ? (
                          // The line keeps its height with no game, so the band does not jump.
                          <ZoneCaps>{"\u00a0"}</ZoneCaps>
                        ) : factLines.map(([label, value, known]) => (
                          <span className="zone-games-fact" key={label}>
                            <ZoneCaps ink="hud">{label}</ZoneCaps>
                            {value === undefined ? null : <ZoneCaps fact={known}>{value}</ZoneCaps>}
                          </span>
                        ))}
                      </span>
                    </span>
                  </span>
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
                      onFocus={id => report({ _tag: "FocusedAction", index: actions.findIndex(action => action.id === id) })}
                      onPress={id => {
                        if (id === OPTS_ID) {
                          onEvent({ _tag: "ChoseGameOpts" })
                          return
                        }
                        const launch = launches.find(candidate => candidate.id === id)
                        if (launch !== undefined) onEvent({ _tag: "ChoseLaunch", gameId: game.id, locationId: launch.locationId })
                      }}
                    />
                  ) : null}
                </div>
              </>
            )}
            {opts !== undefined && game !== undefined ? (
              <div className="zone-games-opts" data-block-exit="" ref={sheet}>
                <ZoneGround ground="gameopts" />
                <div className="zone-games-opts-head">
                  <ZonePanel>
                    <ZoneCaps size="caps-2">Game opts.</ZoneCaps>
                  </ZonePanel>
                  <span className="zone-games-opts-game">
                    <ZoneCaps fact>{opts.gameTitle}</ZoneCaps>
                  </span>
                </div>
                <div
                  aria-label={opts.gameTitle}
                  className="zone-games-opts-rows"
                  data-games-region="opts"
                  inert={opts.asking !== undefined}
                  role="group"
                >
                  {optRows.map(row => (
                    <ZoneOptionRow
                      description={row.description}
                      disabled={row.disabled}
                      flanked
                      focused={row.id === optRows[opts.row]?.id}
                      interaction={row.interaction}
                      key={row.id}
                      label={row.label}
                      onFocus={() => report({ _tag: "FocusedGameOpt", index: optRows.findIndex(candidate => candidate.id === row.id) })}
                      onPress={() => {
                        const action = opts.actions.find(candidate => candidate.id === row.id)
                        if (action === undefined) return
                        onEvent({
                          _tag: "ChoseGameAction",
                          gameId: game.id,
                          actionId: action.id,
                          enabled: action.enabled,
                          destructive: action.destructive === true,
                        })
                      }}
                      value={row.value}
                    />
                  ))}
                </div>
                {opts.asking === undefined ? null : (
                  <div className="zone-games-ask" data-games-region="answer">
                    <div className="zone-games-dialog">
                      <ZoneDialog
                        actions={[
                          { label: "No", onPress: () => onEvent({ _tag: "AnsweredGameAction", yes: false }) },
                          { label: "Yes", onPress: () => onEvent({ _tag: "AnsweredGameAction", yes: true }) },
                        ]}
                        focused={opts.answer}
                        headline={opts.asking.label}
                        onFocusAction={index => report({ _tag: "FocusedAnswer", index })}
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
                    headline="Games"
                    lines={[catalog.message]}
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
      {finding && keysAt === "screen" ? keyboard : null}
      </div>
      {idle === undefined ? null : (
        <div aria-live="off" className="zone-games-idle">
          <ZoneAttract
            act={idle.act}
            art={idle.art}
            clock={clock}
            plays={idle.plays}
            system={idle.system}
            title={idle.title}
            turn={idle.key}
          />
        </div>
      )}
      {tally === undefined ? null : (
        <div className="zone-games-tally" data-games-region="tally">
          <ZoneTally
            counted={tally.counted}
            focused={holding}
            gameId={tally.gameId}
            leaving={tally.leaving}
            onPress={() => onEvent({ _tag: "PressedTally" })}
            plays={tally.plays}
            seconds={tally.seconds}
            title={tally.title}
            total={tally.total}
          />
        </div>
      )}
    </div>
  )
}
