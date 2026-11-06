/**
 * What main.slint draws, from Zone's state and Korri's model: one JSON
 * object whose keys are the Surface's (and the Overlay's) properties
 * (contract 3.1). Words come in Zone's own case: main.slint sets Zone's
 * words in capitals in its caps faces and keeps Korri's facts as written.
 *
 * Pure: no clock, no randomness. The day under LAST PLAYED is computed
 * from Korri's epoch milliseconds in UTC.
 */
import type { SurfaceGame, SurfaceModel, SurfaceNetworkConnection, SurfaceStatus, SurfaceSteps } from "../../../../../contracts/generated/surface"
import { keyAt, keyGrid } from "./keys"
import { formatZoneBytes, zoneProgressShare } from "./plan/bytes"
import { entryHeading, optionsPlan } from "./plan/options"
import { launchPlan } from "./plan/launch-settings"
import { zoneTallyRows, zoneTallyValue } from "./plan/tally"
import { qrDataUrl } from "./qr"
import { actionButtons, barsOf, btn, launchRows, optionRows, row, runWithRows, storeRows, tag, type Btn, type Row, type Tag } from "./rows"
import { launchChoices, playtime, ZONE_FIND_MAX, type ZoneFindLayer, type ZoneLaunchChoice } from "./state/games"
import { IDENTITY_FIXED_CARDS, identityCardCount } from "./state/identity"
import { bandGames, topLayer, ZONE_TABS, type ZoneTab } from "./state/navigation"
import { deviceNameSetting, onboardingTargets, seatTarget, TARGET, ZONE_ONBOARDING_STEPS } from "./state/onboarding"
import { overlayControls, shownRangeValue } from "./state/overlay"
import { problemButtons } from "./state/status"
import { withLook, zoneShown, type ZoneShown, type ZoneSurfaceState } from "./state/surface"
import { focusView, targetRows } from "./focus"
import type { ZoneScreen } from "./plan/screen"
import { dialogRoom } from "./plan/dialog-words"

const TAB_WORDS: Readonly<Record<ZoneTab, string>> = { games: "Games", options: "Options", savedata: "Save data" }
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]

/** A day from epoch ms, in UTC, without Date (Hinnant's civil_from_days): 19 Oct 2026. */
export function dayOf(at: number): string {
  const z = Math.floor(at / 86_400_000) + 719_468
  const era = Math.floor(z / 146_097)
  const doe = z - era * 146_097
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36_524) - Math.floor(doe / 146_096)) / 365)
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100))
  const mp = Math.floor((5 * doy + 2) / 153)
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1
  const m = mp < 10 ? mp + 3 : mp - 9
  const y = yoe + era * 400 + (m <= 2 ? 1 : 0)
  return `${d} ${MONTHS[m - 1] ?? ""} ${y}`
}

export const hoursAndMinutes = ({ hours, minutes }: { readonly hours: number; readonly minutes: number }): string =>
  hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`

/** ZoneGames.tsx `launchLabel`: PLAY, CONTINUE, PLAY HERE, PLAY ON ZAO. */
export const launchLabel = (launch: ZoneLaunchChoice): string => {
  const verb = launch.verb === "continue" ? "Continue" : "Play"
  if (launch.place === undefined) return verb
  return launch.place.here ? `${verb} here` : `${verb} on ${launch.place.label}`
}

/** A card's tags (ZoneCard.tsx `cardTags`, use-zone-games.ts `placesOf`). */
function tagsOf(game: SurfaceGame): Tag[] {
  const locations = game.launchLocations ?? []
  const places = locations.length === 0 ? [{ label: "Here", here: true }]
    : locations.length === 1 ? [{ label: locations[0]!.label, here: false }]
    : locations.map((location, index) => ({ label: location.label, here: index === 0 }))
  return [
    ...(game.subtitle === undefined ? [] : [tag(game.subtitle, "system", true)]),
    ...places.map(place => (place.here ? tag("Here", "here") : tag(place.label, "remote", true))),
    ...(game.resumable === true ? [tag("Continue", "continue")] : []),
    ...(game.download === undefined ? [] : [tag(game.download.bytes === undefined ? "Download" : `Download ${formatZoneBytes(game.download.bytes)}`, "download")]),
  ]
}

/** ZoneCoverWell.tsx `initialsOf`: "The Legend of Zelda" reads LZ. */
export const initialsOf = (title: string): string =>
  title.split(/\s+/)
    .filter(word => word.length > 0 && !/^(?:the|of|a|an|and)$/i.test(word))
    .map(word => word[0] ?? "")
    .join("")
    .slice(0, 2)
    .toUpperCase()

/** ZoneCoverWell.tsx `stripeSetOf`: which of the six stripe sets a well with no art wears. */
export function stripeSetOf(title: string): number {
  let sum = 0
  for (const character of title) sum += character.codePointAt(0) ?? 0
  return sum % 6
}

function signalOf(connection: SurfaceNetworkConnection | undefined) {
  if (connection === undefined) return { shown: false, name: "", korri: false, bars: 0, strength: "" }
  const strength = connection._tag === "Wifi" ? connection.strengthPercent : connection._tag === "Wired" ? 100 : undefined
  return {
    shown: true,
    name: connection._tag === "Wifi" ? connection.ssid : connection._tag === "Wired" ? "Wired" : "Offline",
    korri: connection._tag === "Wifi",
    bars: barsOf(strength, connection._tag !== "Offline"),
    strength: connection._tag === "Wifi" && connection.strengthPercent !== undefined ? `${connection.strengthPercent}%` : "",
  }
}

/**
 * A game's card, made once per game object Korri publishes, and kept across
 * publishes while the facts it shows are the same: each move along the band
 * brings a new model (FocusGame), with every game a new object.
 */
const cards = new WeakMap<SurfaceGame, ReturnType<typeof makeCard>>()
const cardsById = new Map<string, { readonly game: SurfaceGame; readonly made: ReturnType<typeof makeCard> }>()
const sameCardFacts = (a: SurfaceGame, b: SurfaceGame): boolean =>
  a.title === b.title && a.coverArtUrl === b.coverArtUrl && a.subtitle === b.subtitle && a.section === b.section
  && a.resumable === b.resumable && a.download?.bytes === b.download?.bytes && (a.download === undefined) === (b.download === undefined)
  && JSON.stringify(a.launchLocations ?? null) === JSON.stringify(b.launchLocations ?? null)
const card = (game: SurfaceGame) => {
  let made = cards.get(game)
  if (made === undefined) {
    const known = cardsById.get(game.id)
    made = known !== undefined && sameCardFacts(known.game, game) ? known.made : makeCard(game)
    cards.set(game, made)
    if (cardsById.size > 512) cardsById.clear()
    cardsById.set(game.id, { game, made })
  }
  return made
}

const makeCard = (game: SurfaceGame) => ({
  title: game.title,
  ...(game.coverArtUrl === undefined ? {} : { art: { $image: game.coverArtUrl } }),
  hasArt: game.coverArtUrl !== undefined,
  initials: initialsOf(game.title),
  stripes: stripeSetOf(game.title),
  tags: tagsOf(game),
  section: game.section ?? "",
})

/** The game a status names: Korri's title, else the catalog window's. */
function gameTitleOf(korri: SurfaceModel, gameId: string | undefined, given?: string): string {
  if (given !== undefined) return given
  if (gameId === undefined || korri.catalog._tag !== "Ready") return ""
  return korri.catalog.window.find(game => game.id === gameId)?.title ?? ""
}

const EMPTY_KEYBOARD = {
  shown: false, title: "", name: "", draft: "", caret: 0, secret: false, count: "", placeholder: "", danger: "", rows: 5, keys: [] as unknown[],
}

/** A keyboard over a page, in ZoneKeyboard's terms. */
function keyboardView(fields: {
  title: string
  name?: string
  draft: string
  caret: number
  secret: boolean
  layer: ZoneFindLayer
  focused: string | undefined
  maxLength?: number
  placeholder?: string
  danger?: string
  clear?: string
}) {
  const grid = keyGrid(fields.layer, fields.clear)
  const lit = keyAt(grid, fields.focused).id
  const shown = fields.secret ? "•".repeat(fields.draft.length) : fields.draft
  return {
    shown: true,
    title: fields.title,
    name: fields.name ?? "",
    draft: shown,
    caret: Math.max(0, Math.min(fields.caret, shown.length)),
    secret: fields.secret,
    count: fields.maxLength === undefined ? "" : `${fields.draft.length}/${fields.maxLength}`,
    placeholder: fields.draft === "" ? (fields.placeholder ?? "") : "",
    danger: fields.danger ?? "",
    rows: 1 + Math.max(...grid.map(key => key.row)),
    keys: grid.map(key => ({
      label: key.label,
      fact: key.fact,
      row: key.row,
      at: key.at,
      units: key.units,
      focused: key.id === lit,
      current: key.key.kind === "layer" && key.key.layer === fields.layer,
    })),
  }
}

const EMPTY_DIALOG = { shown: false, headline: "", korri: false, lines: [] as string[], title: "", buttons: [] as Btn[] }

/**
 * The sky dialog shown now, before the plan's window of its lines, and the
 * height it stands in (src/plan/dialog-words.ts): press.ts moves its lines.
 */
export function shownDialog(state: ZoneSurfaceState, korri: SurfaceModel, screen: ZoneScreen) {
  if (zoneShown(state, korri) === "paused") return { dialog: pausedView(state, korri).dialog, room: dialogRoom(screen, true, false) }
  const raw = surfaceView(state, korri, screen)
  return { dialog: raw.dialog, room: dialogRoom(screen, false, raw.foot.shown) }
}

function dialogView(headline: string, korri: boolean, lines: readonly string[], title: string, buttons: Btn[]) {
  return { shown: true, headline, korri, lines: lines.filter(line => line !== ""), title, buttons }
}

const EMPTY_BUSY = { shown: false, kicker: "", detail: "", game: "", buttons: [] as Btn[], meter: false, share: 0, sweep: false, figures: "", foot: true, steps: 0, step: 0, stepWords: "" }

/** A launch's named steps (amendment A4): how many, the one running now, and its line. */
export function zoneSteps(steps: SurfaceSteps | undefined): { steps: number; step: number; stepWords: string } {
  if (steps === undefined || steps.labels.length === 0) return { steps: 0, step: 0, stepWords: "" }
  const step = Math.min(Math.max(0, steps.current), steps.labels.length - 1)
  return { steps: steps.labels.length, step, stepWords: `Step ${step + 1} of ${steps.labels.length} · ${steps.labels[step]}` }
}

/** Korri's status as the layer over the page (ZoneStatusLayer). */
function statusView(state: ZoneSurfaceState, korri: SurfaceModel) {
  const status: SurfaceStatus = korri.status
  const focus = state.status.focus
  let busy = EMPTY_BUSY
  let dialog = EMPTY_DIALOG
  let hud = { shown: false, kicker: "", title: "" }
  switch (status._tag) {
    case "Busy": {
      const progress = status.progress
      const share = progress === undefined ? undefined : zoneProgressShare(progress)
      const figures = progress === undefined ? ""
        : status.detail !== undefined ? status.detail
        : progress.total !== undefined ? `${formatZoneBytes(progress.done)} / ${formatZoneBytes(progress.total)}`
        : progress.done > 0 ? formatZoneBytes(progress.done) : ""
      busy = {
        shown: true,
        kicker: status.kicker,
        detail: progress === undefined ? (status.detail ?? "") : "",
        game: gameTitleOf(korri, status.gameId),
        buttons: (status.actions ?? []).map((action, index) => btn(action.label, { focused: index === focus, lit: index === focus, disabled: !action.enabled, danger: action.destructive === true })),
        meter: progress !== undefined,
        share: share ?? 0,
        sweep: progress !== undefined && share === undefined,
        figures,
        foot: true,
        ...zoneSteps(status.steps),
      }
      break
    }
    case "Problem":
      dialog = dialogView(status.kicker, true, [status.reason], gameTitleOf(korri, status.gameId, status.gameTitle),
        problemButtons(status).map((which, index) => btn(which === "dismiss" ? "Dismiss" : "Retry", { focused: index === focus })))
      break
    case "Confirm":
      dialog = dialogView(status.kicker, true, [status.detail ?? ""], gameTitleOf(korri, status.gameId),
        status.actions.map((action, index) => btn(action.label, { focused: index === focus, disabled: !action.enabled, danger: action.destructive === true })))
      break
    case "ChooseSave": {
      const answers = [
        ...status.sides.map(side => `${side.device} · ${dayOf(side.endedAt)} · ${hoursAndMinutes(playtime(side.durationSeconds))}`),
        ...status.actions.map(action => action.label),
      ]
      dialog = dialogView(status.kicker, true, [status.detail ?? ""], gameTitleOf(korri, status.gameId),
        answers.map((label, index) => btn(label, { focused: index === focus })))
      break
    }
    case "Paused":
      dialog = dialogView(status.kicker, true, [status.detail ?? ""], gameTitleOf(korri, status.gameId),
        status.actions.map((action, index) => btn(action.label, { focused: index === focus, disabled: !action.enabled })))
      break
    case "Ended":
      dialog = dialogView(status.kicker, true, [status.reason], gameTitleOf(korri, status.gameId, status.gameTitle), [btn("Dismiss", { focused: true })])
      break
    case "Running":
      hud = { shown: true, kicker: status.kicker, title: gameTitleOf(korri, status.gameId) }
      break
    case "Browsing":
      break
  }
  return { busy, dialog, hud }
}

const GROUND: Readonly<Record<ZoneShown, string>> = {
  games: "games", options: "options", savedata: "savedata", paused: "paused", join: "options", onboarding: "games",
  runwith: "options", launch: "options", store: "options",
}

const BANNER: Readonly<Partial<Record<ZoneShown, string>>> = { runwith: "Run with", launch: "Launch settings", store: "Get games" }
const STEP_WORDS = { controllers: "Controllers", wifi: "Wi-Fi", name: "Name", identity: "Identity" } as const
const RUN_TAGS = { Loading: "continue", Ready: "system", Stale: "open", Warnings: "open", Busy: "here", Conflict: "remote", Error: "remote" } as const
const LAUNCH_TAGS = { Loading: "continue", Ready: "system", Conflict: "remote", Error: "remote" } as const

/** The Surface's view for one window: Zone's view, then its regions, exits and the plan's decisions (src/focus.ts). */
export function view(state: ZoneSurfaceState, korri: SurfaceModel, screen: ZoneScreen) {
  return focusView(surfaceView(state, korri, screen), state, korri, screen)
}

/**
 * The view after the attract's next game, from the view before it: the
 * attract's game and the cards (the new game's card in, the last one out).
 * Everything else is as it was; logic.ts uses it when only that changed.
 */
export function attractStep<V extends { attractCard: number; cards: unknown[]; attract: boolean }>(before: V, state: ZoneSurfaceState, korri: SurfaceModel, screen: ZoneScreen): V {
  const nav = state.nav
  const band = bandGames(nav, korri)
  const attractAt = Math.min(nav.idleCard, Math.max(0, band.length - 1))
  return { ...before, attractCard: attractAt, cards: bandCards(band, nav.card, screen, attractAt, before.attract === true) }
}

/** A mark out of the pool: a focus target in its place, in the band (src/focus.ts), with no words. Shared: never written. */
const OFF_MARK = Object.freeze({ focused: false, focusRegion: "band" })

/**
 * The band's marks, already in their region (src/focus.ts marks only the
 * focused card's as active): the pool's with their game's words, the rest
 * OFF_MARK.
 */
function bandMarks(band: readonly SurfaceGame[], at: number, cardFocused: boolean, screen: ZoneScreen) {
  const reach = poolReach(screen)
  const marks = new Array<Record<string, unknown>>(band.length)
  for (let index = 0; index < band.length; index++) {
    marks[index] = Math.abs(index - at) <= reach
      ? { label: band[index]!.title, focused: index === at && cardFocused, focusRegion: "band" }
      : OFF_MARK
  }
  return marks
}

/** ZoneCardBand's pool either side of the focus: its reach and the next card each way. */
const poolReach = (screen: ZoneScreen): number => (screen.select.band === "wide" ? 2 : 1) + 1

/**
 * The band's cards as the view sends them: the pool's and the attract's
 * whole, the rest NO_CARD, and none past the pool (and the attract's game)
 * but one: ZoneCardBand and ZoneAttract read no card further on, and the
 * one past the pool is the spare slot's until it takes its next card.
 */
function bandCards(band: readonly SurfaceGame[], at: number, screen: ZoneScreen, attractAt: number, attracting: boolean) {
  const reach = poolReach(screen)
  const end = Math.min(band.length, Math.max(at + reach + 2, attracting ? attractAt + 1 : 0))
  const cards = new Array<unknown>(end)
  for (let index = 0; index < end; index++) {
    cards[index] = Math.abs(index - at) <= reach || (attracting && index === attractAt) ? card(band[index]!) : NO_CARD
  }
  return cards
}

/**
 * A card the band and the attract do not draw now: no words, no art. The
 * shell keeps a row's fields a view leaves out, so the row stays as it was
 * and the view stays small (each view call writes its JSON whole).
 */
const NO_CARD = {}

function surfaceView(state: ZoneSurfaceState, korri: SurfaceModel, screen: ZoneScreen) {
  const page = zoneShown(state, korri)
  const nav = state.nav
  const band = bandGames(nav, korri)
  const game = band[nav.card]
  const layer = topLayer(nav)
  const onGames = page === "games"

  // ---- GAMES ----
  const launches = game === undefined ? [] : launchChoices(game)
  const choice = launches.length > 1
  const actionWords = [...launches.map(launchLabel), "Opts."]
  const focusedAction = onGames && nav.zone === "actions" && layer === "none" ? Math.min(nav.action, actionWords.length - 1) : -1
  const lit = choice ? (focusedAction >= 0 && focusedAction < launches.length ? focusedAction : -1) : 0
  const facts = game === undefined ? [] : [
    { label: "Last played", value: game.lastPlayedAt === undefined ? "--" : dayOf(game.lastPlayedAt), known: game.lastPlayedAt !== undefined },
    { label: "Plays", value: game.playCount === undefined ? "--" : String(game.playCount), known: game.playCount !== undefined },
    {
      label: "Time",
      value: game.totalPlaytimeSeconds === undefined ? "--'--\"--" : hoursAndMinutes(playtime(game.totalPlaytimeSeconds)),
      known: game.totalPlaytimeSeconds !== undefined,
    },
    ...(game.download === undefined ? [] : [{ label: "Download", value: game.download.bytes === undefined ? "" : formatZoneBytes(game.download.bytes), known: true }]),
    ...(game.activeProfile === undefined ? [] : [{ label: "Profile", value: game.activeProfile, known: true }]),
  ]
  const catalog = korri.catalog

  // ---- Layers over the page ----
  const status = statusView(state, korri)
  let keyboard: ReturnType<typeof keyboardView> | typeof EMPTY_KEYBOARD = EMPTY_KEYBOARD
  let dialog: ReturnType<typeof dialogView> | typeof EMPTY_DIALOG = EMPTY_DIALOG
  let rows: Row[] = []
  let head = { shown: false, title: "", tags: [] as Tag[] }
  let well = { shown: false, initials: "", stripes: 0 }

  if (onGames && layer === "keyboard") {
    keyboard = keyboardView({ title: "Find", draft: nav.find.draft, caret: nav.find.caret, secret: false, layer: nav.find.layer, focused: nav.find.key, maxLength: ZONE_FIND_MAX })
  }
  if (onGames && catalog._tag === "Error" && layer === "none") {
    dialog = dialogView("Games", false, [catalog.message], "", [btn("Reload", { focused: nav.zone !== "tabs" })])
  }

  // GAME OPTS.: Korri's actions for the focused game, from FocusGame.
  const optsShown = onGames && layer === "gameopts"
  let optsRows: Row[] = []
  if (optsShown && game !== undefined) {
    const actions = korri.focusedGame?.gameId === game.id ? korri.focusedGame.actions : []
    optsRows = actions.length === 0 ? [row({ kind: "word", label: "Loading" })] : actions.flatMap((action, index) => {
      const focused = index === nav.opts.row && nav.opts.asking === undefined
      return [
        row({ kind: "item", label: action.label, value: "Run", focused, disabled: !action.enabled, danger: action.destructive === true, choice: true }),
        ...(action.description !== undefined && (focused || !action.enabled) ? [row({ kind: "note", label: action.description, labelFact: true })] : []),
      ]
    })
    const asking = nav.opts.asking
    const asked = asking === undefined ? undefined : actions.find(action => action.id === asking.actionId)
    if (asked !== undefined) {
      dialog = dialogView(asked.label, false, [asked.description ?? ""], game.title,
        [btn("No", { focused: nav.opts.answer === 0 }), btn("Yes", { focused: nav.opts.answer === 1 })])
    }
  }

  const slices = withLook(state, korri)
  switch (page) {
    case "options":
    case "join": {
      rows = optionRows(optionsPlan(slices), state.options)
      const board = state.options.keyboard
      if (board !== undefined) {
        const heading = entryHeading(board.entry)
        const attempt = korri.network?.wifi._tag === "Available" ? korri.network.wifi.attempt : undefined
        keyboard = keyboardView({
          title: heading.title,
          ...(heading.name === undefined ? {} : { name: heading.name }),
          draft: board.draft,
          caret: board.caret,
          secret: heading.secret,
          layer: board.layer,
          focused: board.focusedKey,
          ...(heading.maxLength === undefined ? {} : { maxLength: heading.maxLength }),
          ...(heading.placeholder === undefined ? {} : { placeholder: heading.placeholder }),
          ...(attempt?._tag === "Failed" && board.entry._tag !== "Setting" ? { danger: attempt.message } : {}),
          ...(heading.clearLabel === undefined ? {} : { clear: heading.clearLabel }),
        })
      }
      const asking = state.options.dialog
      if (asking !== undefined) {
        dialog = dialogView(asking.confirm.headline, asking.confirm.korriHeadline === true, asking.confirm.lines, "",
          [btn("No", { focused: asking.focused === 0 }), btn(asking.confirm.confirmLabel, { focused: asking.focused === 1 })])
      }
      break
    }
    case "launch": {
      const launch = korri.launchSettings
      if (launch === undefined || launch._tag === "Closed") break
      rows = launchRows(state, korri, launch)
      head = { shown: true, title: launch.gameTitle ?? "", tags: [tag(launch._tag, LAUNCH_TAGS[launch._tag])] }
      const board = state.launch.keyboard
      if (board !== undefined) {
        const heading = entryHeading(board.entry)
        keyboard = keyboardView({
          title: heading.title,
          draft: board.draft,
          caret: board.caret,
          secret: heading.secret,
          layer: board.layer,
          focused: board.focusedKey,
          ...(heading.maxLength === undefined ? {} : { maxLength: heading.maxLength }),
          ...(heading.placeholder === undefined ? {} : { placeholder: heading.placeholder }),
          ...(heading.clearLabel === undefined ? {} : { clear: heading.clearLabel }),
        })
      }
      const asking = state.launch.dialog
      if (asking !== undefined) {
        dialog = dialogView(asking.confirm.headline, asking.confirm.korriHeadline === true, asking.confirm.lines, "",
          [btn("No", { focused: asking.focused === 0 }), btn(asking.confirm.confirmLabel, { focused: asking.focused === 1 })])
      }
      void launchPlan
      break
    }
    case "runwith": {
      const runner = korri.runnerChoice
      if (runner === undefined || runner._tag === "Closed") break
      rows = runWithRows(state, runner)
      head = { shown: true, title: runner.gameTitle, tags: [tag(runner._tag, RUN_TAGS[runner._tag])] }
      break
    }
    case "store": {
      const shape = storeRows(state, korri)
      rows = shape.rows
      head = { shown: shape.title !== "" || shape.tags.length > 0, title: shape.title, tags: [...shape.tags] }
      well = { shown: shape.well, initials: initialsOf(shape.title), stripes: stripeSetOf(shape.title) }
      const search = state.store.keyboard
      if (search !== undefined) {
        keyboard = keyboardView({ title: "Search", draft: search.draft, caret: search.caret, secret: false, layer: search.layer, focused: search.key, maxLength: 40 })
      }
      if (shape.dialog.length > 0) {
        const at = Math.min(state.store.dialog, shape.dialog.length - 1)
        dialog = dialogView(shape.dialogHeadline, shape.dialogKorri, [shape.dialogLine], "",
          shape.dialogActions.map((action, index) => btn(action.label, { focused: index === at, disabled: !action.enabled, danger: action.destructive === true })))
      }
      // Work under way on the open entry: the busy strip at the body's foot.
      const details = korri.store?.details
      if (state.store.page === "entry" && details?._tag === "Ready" && details.get._tag === "Working" && !status.busy.shown) {
        const get = details.get
        const progress = get.progress
        const share = progress === undefined ? undefined : zoneProgressShare(progress)
        status.busy = {
          shown: true,
          kicker: get.kicker,
          detail: progress === undefined ? (get.detail ?? "") : "",
          game: "",
          buttons: get.actions.map(action => btn(action.label, { focused: true, lit: true, disabled: !action.enabled })),
          meter: progress !== undefined,
          share: share ?? 0,
          sweep: progress !== undefined && share === undefined,
          figures: get.detail ?? (progress?.total !== undefined ? `${formatZoneBytes(progress.done)} / ${formatZoneBytes(progress.total)}` : ""),
          foot: true,
          ...zoneSteps(undefined),
        }
      }
      break
    }
    case "onboarding":
      rows = onboardingRows(state, korri)
      if (state.onboarding.entry !== undefined) {
        const entry = state.onboarding.entry
        const setting = deviceNameSetting(korri)
        const max = setting?.interaction?.kind === "text" ? setting.interaction.maxLength : undefined
        const placeholder = setting?.interaction?.kind === "text" ? setting.interaction.placeholder : undefined
        keyboard = keyboardView({
          title: "Device",
          draft: entry.draft,
          caret: entry.caret,
          secret: false,
          layer: entry.layer,
          focused: entry.focusedKey,
          ...(max === undefined ? {} : { maxLength: max }),
          ...(placeholder === undefined ? {} : { placeholder }),
        })
      }
      break
    case "savedata":
      break
    case "games":
    case "paused":
      break
  }
  if (status.dialog.shown) dialog = status.dialog

  // ---- SAVE DATA ----
  const save = saveView(state, korri, page === "savedata")
  if (page === "savedata") {
    if (save.keyboard !== undefined) keyboard = save.keyboard
    if (save.dialog !== undefined && !status.dialog.shown) dialog = save.dialog
    if (save.busy !== undefined && !status.busy.shown) status.busy = save.busy
  }

  // ---- First boot ----
  const boarding = onboardingView(state, korri, page === "onboarding")

  // ---- The moments ----
  const launchCard = nav.launch === undefined || nav.launch.leaving || !onGames
    ? { shown: false, title: "", system: "", act: 0 }
    : { shown: true, title: nav.launch.title, system: nav.launch.system ?? "", act: nav.launch.act }
  const tally = nav.tally === undefined || nav.tally.leaving || !onGames
    ? { shown: false, title: "", counted: false, rows: [] as { label: string; value: string; share: number; kind: string; amount: number }[] }
    : {
        shown: true,
        title: nav.tally.title,
        counted: nav.tally.counted,
        rows: zoneTallyRows(nav.tally).map(entry => ({
          label: entry.label,
          value: zoneTallyValue(entry.kind, nav.tally!),
          share: entry.share,
          // What the value counts up to, for ZoneTally's count-up: seconds for TIME and TOTAL, the count for PLAYS.
          kind: entry.kind,
          amount: entry.kind === "time" ? (nav.tally!.seconds ?? 0) : entry.kind === "plays" ? nav.tally!.plays : (nav.tally!.total ?? 0),
        })),
      }

  const attracting = onGames && layer === "idle" && band.length > 0
  const attractAt = Math.min(nav.idleCard, Math.max(0, band.length - 1))
  const cardFocused = onGames && nav.zone === "cards" && layer === "none" && !status.busy.shown && !dialog.shown
  // On the tabs (or LAUNCH SETTINGS' buttons) a page's list keeps its row as the one it enters on, unfocused.
  if (((page === "options" || page === "savedata") && nav.zone === "tabs") || (page === "launch" && state.launchAction !== undefined)) {
    rows = rows.map(entry => (entry.focused ? { ...entry, focused: false, focusActive: true } : entry))
  }
  const device = deviceNameSetting(korri)?.value ?? ""
  const tabbed = page === "games" || page === "options" || page === "savedata"
  return {
    look: state.look,
    page,
    ground: optsShown ? "gameopts" : GROUND[page],
    tabsShown: tabbed,
    banner: page === "onboarding" ? STEP_WORDS[state.onboarding.step] : (BANNER[page] ?? ""),
    pips: page === "onboarding" ? ZONE_ONBOARDING_STEPS.length : 0,
    pip: ZONE_ONBOARDING_STEPS.indexOf(state.onboarding.step),
    // The tabs hold the cursor unless a keyboard or a modal question covers them (GAMES' catalog error leaves them reachable).
    tabs: ZONE_TABS.map(tab => ({ label: TAB_WORDS[tab], active: tab === nav.tab, focused: tabbed && nav.zone === "tabs" && tab === nav.tabCursor && (!onGames || layer === "none") && !keyboard.shown && (!dialog.shown || (onGames && catalog._tag === "Error" && !status.dialog.shown)) })),
    hints: { back: "Back", confirm: "Confirm" },
    foot: {
      shown: page !== "onboarding" && page !== "paused",
      timeLabel: "Time",
      clock: korri.clockLabel ?? "",
      device,
      signal: page === "store" || page === "runwith" || page === "launch" ? signalOf(undefined) : signalOf(korri.network?.connection),
      build: korri.buildLabel ?? "",
    },
    // GAMES
    zone: onGames ? nav.zone : "",
    catalog: catalog._tag,
    // The cards stay as they were across a move (one object per game); the marks carry the focus.
    // Only the cards drawn or about to be: ZoneCardBand's pool (the focus and
    // reach + 1 either side) and the attract's game; the rest are NO_CARD.
    cards: bandCards(band, nav.card, screen, attractAt, attracting),
    // Every card's mark (each a focus target in its place); the pool's carry
    // their game's words, the rest only their focus, to keep the view small.
    cardMarks: bandMarks(band, nav.card, cardFocused, screen),
    focusedCard: nav.card,
    cardFocused,
    firstIndex: catalog._tag === "Ready" && catalog.query.text.trim().toLocaleLowerCase() === nav.find.draft.trim().toLocaleLowerCase() ? catalog.firstIndex : 0,
    total: catalog._tag === "Ready" && catalog.query.text.trim().toLocaleLowerCase() === nav.find.draft.trim().toLocaleLowerCase() ? catalog.total : band.length,
    facts,
    actions: game === undefined ? [] : actionWords.map((label, index) => btn(label, { lit: index === lit, focused: index === focusedAction })),
    findShown: catalog._tag === "Ready",
    find: btn(catalog._tag === "Ready" ? "Find" : "", { focused: onGames && layer === "none" && (nav.zone === "find" || (nav.zone === "cards" && band.length === 0 && catalog._tag === "Ready")) }),
    kept: nav.find.draft === "" ? "" : nav.find.draft,
    getShown: korri.store !== undefined && catalog._tag !== "Error",
    get: btn(korri.store !== undefined && catalog._tag !== "Error" ? "Get games" : "", { focused: onGames && layer === "none" && (nav.zone === "get" || (nav.zone === "cards" && band.length === 0 && catalog._tag !== "Ready" && korri.store !== undefined)) }),
    typing: onGames && layer === "keyboard",
    // Zone's idle layer (navigation.ts), opened by the shell's Tick: the attract
    // shows the game at idleCard (in catalog order) and moves on with each IdleTicked.
    attract: attracting,
    attractCard: attractAt,
    // GAME OPTS.
    optsShown,
    optsTitle: game?.title ?? "",
    optsRows,
    // List pages
    head,
    rows,
    well,
    // SAVE DATA
    saveCards: save.cards.map((entry, index) => ({ ...entry, focused: save.marked && index === save.focus && state.identity.flow._tag !== "retired" })),
    saveFocus: save.focus,
    saveMarked: save.marked,
    // The cursor is on a retired key's buttons: its card stays marked.
    saveLit: save.marked && state.identity.flow._tag === "retired",
    retired: save.retired,
    qrShown: save.qr !== undefined,
    ...(save.qr === undefined ? {} : { qr: { $image: save.qr } }),
    secret: save.secret,
    slab: btn(save.qr === undefined ? "" : "secret", { focused: save.slab && nav.zone !== "tabs" }),
    // First boot
    seats: boarding.seats,
    seatName: boarding.seatName,
    versus: boarding.versus,
    next: boarding.next,
    newSave: boarding.save,
    // Layers
    keyboard,
    dialog,
    busy: status.busy,
    hud: page === "paused" ? { shown: false, kicker: "", title: "" } : status.hud,
    launchCard,
    tally,
    veil: dialog.shown || (status.busy.shown && korri.status._tag === "Busy"),
  }
}

// ---- SAVE DATA ----

const shortKey = (key: string): string => (key.length > 16 ? `${key.slice(0, 8)}…${key.slice(-8)}` : key)

function saveView(state: ZoneSurfaceState, korri: SurfaceModel, shown: boolean) {
  const identity = korri.identityManagement
  const model = state.identity
  const flow = model.flow
  const statusTag = identity?.status._tag
  const marked = shown && state.nav.zone !== "tabs" && (flow._tag === "idle" || flow._tag === "retired" || (flow._tag === "showSecret" && !flow.onSlab))
  const cards = identity === undefined
    ? [{ header: "Identity", word: "Not available", emblem: "retired", foot: "", footKind: "" }]
    : [
        { header: "Back up", word: "This key", emblem: "key", foot: identity.localBackupAvailable ? "Backed up" : "New", footKind: identity.localBackupAvailable ? "saved" : "open" },
        { header: "Switch", word: "From backup", emblem: "restore", foot: "", footKind: "" },
        { header: "Switch", word: "Bunker", emblem: "link", foot: "", footKind: "" },
        ...identity.retiredPublicKeys.map(key => ({ header: "Retired", word: shortKey(key), emblem: "retired", foot: "", footKind: "" })),
      ]
  const retired = flow._tag === "retired"
    ? [btn("Back up", { focused: flow.action === "backup", lit: true }), btn("Delete", { focused: flow.action === "delete", danger: true })]
    : model.card >= IDENTITY_FIXED_CARDS && identity !== undefined && model.card < identityCardCount(identity)
      ? [btn("Back up", { lit: true }), btn("Delete")]
      : []
  let keyboard: ReturnType<typeof keyboardView> | undefined
  let dialog: ReturnType<typeof dialogView> | undefined
  let busy: typeof EMPTY_BUSY | undefined
  switch (flow._tag) {
    case "password":
      keyboard = keyboardView({ title: "Password", draft: flow.draft.text, caret: flow.draft.caret, secret: true, layer: flow.draft.layer, focused: flow.draft.focusedKey })
      break
    case "secret":
      keyboard = keyboardView({ title: "Backup", draft: flow.draft.text, caret: flow.draft.caret, secret: false, layer: flow.draft.layer, focused: flow.draft.focusedKey })
      break
    case "bunker":
      keyboard = keyboardView({ title: "Bunker", draft: flow.draft.text, caret: flow.draft.caret, secret: false, layer: flow.draft.layer, focused: flow.draft.focusedKey })
      break
    case "disposition":
      dialog = dialogView("Local records", false, ["Transfer them to the new key, or delete them"], "",
        [btn("Transfer", { focused: flow.focus === 0 }), btn("Delete", { focused: flow.focus === 1, danger: true })])
      break
    case "trust":
      dialog = dialogView("Switch key?", false, ["Other devices will stop trusting this key"], "",
        [btn("No", { focused: flow.focus === 0 }), btn("Yes", { focused: flow.focus === 1 })])
      break
    case "confirmDelete":
      dialog = dialogView("Delete this key?", false, ["Back it up first."], shortKey(flow.publicKey),
        [btn("No", { focused: flow.focus === 0 }), btn("Yes", { focused: flow.focus === 1 })])
      break
    case "confirmBackedUp":
      dialog = dialogView("Is it backed up?", false, [], shortKey(flow.publicKey),
        [btn("No", { focused: flow.focus === 0 }), btn("Yes", { focused: flow.focus === 1 })])
      break
    case "idle":
    case "retired":
    case "showSecret":
      break
  }
  if (identity?.status._tag === "Problem") dialog = dialogView("Identity", false, [identity.status.message], "", [btn("Dismiss", { focused: true })])
  if (identity?.status._tag === "Switched") dialog = dialogView("Switched", false, [], shortKey(identity.status.ownerPublicKey), [btn("OK", { focused: true })])
  if (identity?.status._tag === "Working") busy = { ...EMPTY_BUSY, shown: true, kicker: identity.status.operation }
  const backup = identity?.status._tag === "BackupReady" ? identity.status.encryptedSecret : undefined
  return {
    cards,
    focus: model.card,
    marked,
    retired,
    keyboard,
    dialog,
    busy,
    qr: backup === undefined || !shown ? undefined : qrDataUrl(backup),
    secret: backup === undefined ? "" : (backup.match(/.{1,4}/g) ?? []).join(" "),
    slab: flow._tag === "showSecret" && flow.onSlab,
    status: statusTag ?? "",
  }
}

// ---- First boot ----

function onboardingRows(state: ZoneSurfaceState, korri: SurfaceModel): Row[] {
  const model = state.onboarding
  const focused = (target: string) => model.entry === undefined && model.focus === target
  switch (model.step) {
    case "controllers":
    case "identity":
      return []
    case "name": {
      const setting = deviceNameSetting(korri)
      return [
        row({ kind: "title", label: "Device" }),
        row({ kind: "item", label: "Name", value: setting?.value ?? "", valueFact: true, focused: focused(TARGET.name), disabled: setting?.interaction?.kind !== "text" }),
        ...(setting?.description === undefined || !focused(TARGET.name) ? [] : [row({ kind: "note", label: setting.description, labelFact: true })]),
      ]
    }
    case "wifi": {
      const network = korri.network
      const wifi = network?.wifi
      const rows: Row[] = [row({ kind: "title", label: "Wi-Fi" })]
      if (network !== undefined) {
        const connection = network.connection
        rows.push(row({
          kind: "item",
          label: "Connection",
          value: connection._tag === "Wifi" ? connection.ssid : connection._tag === "Wired" ? "Wired" : "Offline",
          valueFact: connection._tag === "Wifi",
          focused: focused(TARGET.connection),
          bars: connection._tag === "Wifi" ? barsOf(connection.strengthPercent) : connection._tag === "Wired" ? 4 : 0,
        }))
      }
      if (wifi?._tag === "Available") {
        for (const item of wifi.networks) {
          rows.push(row({
            kind: "item",
            label: item.ssid,
            labelFact: true,
            value: `${item.strengthPercent}% ${item.secured ? "Locked" : "Open"}${item.known ? " Saved" : ""}`,
            focused: focused(`net:${item.id}`),
            saving: wifi.attempt._tag === "Connecting" && wifi.attempt.ssid === item.ssid,
          }))
        }
        rows.push(row({ kind: "item", label: "Networks", value: wifi.scanning ? "Scanning…" : "Scan", focused: focused(TARGET.scan) }))
      }
      rows.push(row({ kind: "item", label: "Skip", value: "Later", focused: focused(TARGET.skip) }))
      return rows
    }
  }
}

function onboardingView(state: ZoneSurfaceState, korri: SurfaceModel, shown: boolean) {
  const model = state.onboarding
  const controllers = korri.onboarding?.controllers ?? []
  const heard = controllers.filter(controller => controller.heard)
  const seats = model.step !== "controllers" || !shown ? [] : (controllers.length === 0 ? [{ id: "", name: "", heard: false }] : controllers).map((controller, index) => ({
    seat: controller.heard ? `${index + 1}P` : "",
    name: controller.name,
    heard: controller.heard,
    // Only a heard controller's seat takes the cursor.
    ...(controller.heard ? { focused: model.entry === undefined && model.focus === seatTarget(controller.id) } : {}),
  }))
  const focusedSeat = controllers.find(controller => model.focus === seatTarget(controller.id))
  const targets = onboardingTargets(model, korri)
  const nextTarget = model.step === "identity" ? TARGET.finish : TARGET.next
  const canGo = model.step === "identity" ? model.started : targets.includes(TARGET.next)
  return {
    seats,
    seatName: focusedSeat?.name ?? "",
    versus: controllers.length === 2 && heard.length === 2,
    next: shown
      ? btn(model.step === "identity" ? "Finish" : "Next", { focused: model.entry === undefined && model.focus === nextTarget, lit: canGo, disabled: !canGo })
      : btn(""),
    save: { shown: shown && model.step === "identity", focused: model.focus === TARGET.save, started: model.started },
  }
}

// ---- PAUSED ----

/** The Overlay's view: Korri's title on the red band, the controls in the yellow wedge. */
export function overlayView(state: ZoneSurfaceState, korri: SurfaceModel, screen: ZoneScreen) {
  const raw = pausedView(state, korri)
  const dialog = raw.dialog
  // The Overlay's regions: a question's buttons, and PAUSED's controls. The
  // controls step down-left like stairs, so each control is its own region:
  // up and down go to the control above and below, left and right step its
  // value or stay.
  const planned = focusView({ actions: [], retired: [], dialog, busy: raw.busy, rows: [], optsRows: [], tabs: [] }, state, korri, screen, true)
  const rows = targetRows(raw.rows as unknown as Record<string, unknown>[])
  const controls = rows.flatMap((entry, index) => ("focused" in entry ? [index] : []))
  const region = (index: number) => `control-${controls.indexOf(index)}`
  const focusPlan: Record<string, Record<string, { to: string; entry: "first" }>> = {}
  if (!dialog.shown && !(raw.busy.shown && raw.busy.buttons.length > 0)) {
    controls.forEach((_, k) => {
      focusPlan[`control-${k}`] = {
        ...(k > 0 ? { up: { to: `control-${k - 1}`, entry: "first" as const } } : {}),
        ...(k < controls.length - 1 ? { down: { to: `control-${k + 1}`, entry: "first" as const } } : {}),
      }
    })
  }
  return {
    ...raw,
    dialog: planned.dialog,
    busy: planned.busy,
    rows: rows.map((entry, index) => ("focused" in entry ? { ...entry, focusRegion: region(index) } : entry)),
    screen: planned.screen,
    focusPlan,
  }
}

function pausedView(state: ZoneSurfaceState, korri: SurfaceModel) {
  const overlay = korri.presentation
  const empty = { look: state.look, heading: "Paused", korriHeading: false, hasArt: false, rows: [] as Row[], focusAt: 0, dialog: EMPTY_DIALOG, busy: EMPTY_BUSY, hints: { back: "Back", confirm: "Confirm" } }
  if (overlay.kind !== "gameplay-overlay") return empty
  const model = state.overlay
  const rows: Row[] = []
  const controlRow = (control: (typeof overlay.controls)[number]): Row[] => {
    const focused = control.id === model.focusedId && model.asking === undefined
    const interaction = control.interaction
    const value = interaction.kind === "toggle" ? (interaction.value ? interaction.trueLabel : interaction.falseLabel)
      : interaction.kind === "choice" ? (interaction.options.find(option => option.value === interaction.value)?.label ?? interaction.value)
      : interaction.kind === "range" ? String(shownRangeValue(model, control) ?? interaction.value) : ""
    const share = interaction.kind === "range" ? ((shownRangeValue(model, control) ?? interaction.value) - interaction.min) / Math.max(1e-9, interaction.max - interaction.min) : -1
    const note = !control.enabled ? control.disabledReason ?? control.description : focused ? control.description : undefined
    return [
      row({
        kind: interaction.kind === "range" ? "range" : "item",
        label: control.label,
        value,
        valueFact: true,
        focused,
        disabled: !control.enabled,
        danger: control.destructive,
        choice: interaction.kind !== "command",
        bars: interaction.kind === "range" ? Math.round(share * 100) : -1,
        saving: interaction.kind === "range" && model.moving[control.id] !== undefined,
      }),
      ...(note === undefined ? [] : [row({ kind: "note", label: note, labelFact: true })]),
    ]
  }
  for (const control of overlay.controls) rows.push(...controlRow(control))
  for (const group of overlay.groups) {
    rows.push(row({ kind: "title", label: group.label }))
    for (const control of group.controls) rows.push(...controlRow(control))
  }
  const asking = model.asking
  const asked = asking === undefined ? undefined : overlayControls(overlay).find(control => control.id === asking.controlId)
  const status = statusView(state, korri)
  const dialog = status.dialog.shown ? status.dialog
    : asked === undefined ? EMPTY_DIALOG
    : dialogView(asked.label, false, [asked.description ?? ""], "", [btn("No", { focused: model.answer === 0 }), btn("Yes", { focused: model.answer === 1 })])
  const running = korri.status._tag === "Running" && korri.status.gameId !== undefined && korri.catalog._tag === "Ready"
    ? korri.catalog.window.find(game => game.id === (korri.status as { gameId?: string }).gameId) : undefined
  return {
    look: state.look,
    heading: overlay.title ?? "Paused",
    korriHeading: overlay.title !== undefined,
    hasArt: running?.coverArtUrl !== undefined,
    ...(running?.coverArtUrl === undefined ? {} : { art: { $image: running.coverArtUrl } }),
    rows,
    focusAt: Math.max(0, rows.findIndex(entry => entry.focused)),
    dialog,
    busy: status.busy,
    hints: { back: "Back", confirm: "Confirm" },
  }
}

export { actionButtons }
