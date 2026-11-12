/**
 * Every screen Korri publishes that is not the library, as a panel: a
 * title, a line under it, rows and buttons, each with what a press does
 * (src/types.ts). One pure function of the state and Korri's model builds a
 * panel; the view draws it and the update answers presses on it.
 *
 *   sheets   the house menu, a set's sort and filter, a game's actions
 *   pages    settings, Wi-Fi, getting games (providers, a search, an entry),
 *            invites, identity and backup
 *   Korri's  first boot, the runner choice, launch settings, a game's saves:
 *            shown while Korri publishes them, closed by Korri's own action
 *   overlay  the gameplay overlay's controls
 *
 * Every string Korri publishes is drawn as it is. Velvet's own words are
 * plain sentences; it never infers what an action means from its label or
 * its id, with the one documented exception that the host's close action
 * of launch settings ends in ":cancel" (the treaty's SurfaceLaunchSettingsContent).
 */
import type {
  SurfaceAction,
  SurfaceGameplayControl,
  SurfaceGameplayOverlayPresentation,
  SurfaceModel as Model,
  SurfaceStoreGet,
} from "../../../../../contracts/generated/surface"
import { gameById, sameQuery } from "./lists"
import { actionRow, connectionLine, connectionWords, question, settingRows, wifiRows, WIFI_PASSWORD_MAX, WIFI_SSID_MAX, numberFits } from "./settings"
import { NONE, send, seq, type Act, type ButtonSpec, type Entry, type GameplayValue, type PageRef, type PanelSpec, type RowSpec, type SheetKind, type State } from "./types"
import { bytes, duration, saved } from "./words"

const run = (actionId: string): Act => send({ _tag: "RunAction", actionId })
const head = (id: string, label: string): RowSpec => ({ id, kind: "head", label })
const note = (id: string, label: string): RowSpec => ({ id, kind: "note", label })
const problem = (id: string, label: string): RowSpec => ({ id, kind: "problem", label })
const CLOSE: Act = { t: "close" }

const actionRows = (prefix: string, actions: readonly SurfaceAction[], after: Act = NONE): RowSpec[] =>
  actions.map(action => actionRow(`${prefix}${action.id}`, action, after.t === "none" ? run(action.id) : seq(run(action.id), after)))

const actionButtons = (prefix: string, actions: readonly SurfaceAction[]): ButtonSpec[] =>
  actions.map(action => ({
    id: `${prefix}${action.id}`, label: action.label, quiet: true,
    ...(action.enabled ? {} : { dim: true }), ...(action.destructive === true ? { danger: true } : {}),
    ...(action.description === undefined ? {} : { note: action.description }),
    press: !action.enabled ? NONE : action.destructive === true ? { t: "ask", ask: question(action.label, action.description === undefined ? [] : [action.description], action.label, run(action.id)) } : run(action.id),
  }))

// ---- Sheets ----

function menuSheet(model: Model): PanelSpec {
  const rows: RowSpec[] = []
  if (model.store !== undefined) rows.push({ id: "m:store", kind: "row", label: "Find games", mark: "more", press: seq(CLOSE, send({ _tag: "OpenStore" }), { t: "open", page: { p: "store" } }) })
  const invites = model.invites ?? []
  if (invites.length > 0) rows.push({ id: "m:invites", kind: "row", label: "Invites", value: String(invites.length), mark: "more", press: seq(CLOSE, { t: "open", page: { p: "invites" } }) })
  rows.push({ id: "m:settings", kind: "row", label: "Settings", mark: "more", press: seq(CLOSE, { t: "open", page: { p: "settings" } }) })
  if (model.network !== undefined) rows.push({ id: "m:wifi", kind: "row", label: "Wi-Fi", value: connectionWords(model.network.connection), mark: "more", press: seq(CLOSE, { t: "open", page: { p: "wifi" } }) })
  if (model.identityManagement !== undefined) rows.push({ id: "m:identity", kind: "row", label: "Identity and backup", mark: "more", press: seq(CLOSE, { t: "open", page: { p: "identity" } }) })
  if (model.actions.length > 0) rows.push(head("m:device", "This device"), ...actionRows("m:a:", model.actions, CLOSE))
  return { key: "sheet:menu", title: "Menu", rows, buttons: [], back: CLOSE, menu: CLOSE }
}

export const SORTS = [["recent", "Recently played"], ["title", "A to Z"], ["mostPlayed", "Most played"]] as const

export const sortWords = (order: string): string => (order === "recent" ? "recently played first" : order === "mostPlayed" ? "most played first" : order === "title" ? "A to Z" : "in Korri's order")

function sortSheet(state: State, model: Model): PanelSpec {
  const set = state.set
  const rows: RowSpec[] = [head("o:sort", "Sort")]
  for (const [order, label] of SORTS) rows.push({ id: `o:${order}`, kind: "row", label, ...(set?.order === order ? { mark: "check" as const } : {}), press: { t: "sort", order } })
  // Every game can be narrowed to one system; a shelf's own set is already one thing.
  const systems = (model.systems ?? []).filter(system => system.total > 0)
  if (set?.key === "every" && systems.length > 0) {
    rows.push(head("o:show", "Show"))
    rows.push({ id: "o:all", kind: "row", label: "Every system", ...(set.systemId === undefined ? { mark: "check" as const } : {}), press: { t: "sort", systemId: null } })
    for (const system of systems) rows.push({ id: `o:sys:${system.systemId}`, kind: "row", label: system.system, value: String(system.total), ...(set.systemId === system.systemId ? { mark: "check" as const } : {}), press: { t: "sort", systemId: system.systemId } })
  }
  return { key: "sheet:sort", title: "Sort and filter", rows, buttons: [], back: CLOSE, first: `o:${set?.order ?? "recent"}` }
}

function actionsSheet(state: State, model: Model): PanelSpec {
  const game = state.detail?.game
  const focused = model.focusedGame
  const rows: RowSpec[] = []
  if (game === undefined) rows.push(note("g:none", "No game is open."))
  else if (focused === undefined || focused.gameId !== game.id) rows.push(note("g:wait", "One moment."))
  else if (focused.actions.length === 0) rows.push(note("g:none", "Korri has nothing more for this game."))
  else {
    for (const action of focused.actions) {
      const act = seq(send({ _tag: "RunGameAction", gameId: game.id, actionId: action.id }), CLOSE)
      rows.push(actionRow(`g:${action.id}`, action, act))
    }
  }
  return { key: "sheet:actions", title: game?.title ?? "More", rows, buttons: [], back: CLOSE }
}

export function sheetPanel(kind: SheetKind, state: State, model: Model): PanelSpec {
  return kind === "menu" ? menuSheet(model) : kind === "sort" ? sortSheet(state, model) : actionsSheet(state, model)
}

// ---- Pages ----

function settingsPage(state: State, model: Model): PanelSpec {
  const rows: RowSpec[] = []
  model.settings.forEach((group, index) => {
    rows.push(head(`s:head:${index}`, group.title))
    for (const item of group.items) rows.push(...settingRows(item, model.settingsStatus, state))
  })
  const places: RowSpec[] = []
  if (model.network !== undefined) places.push({ id: "v:wifi", kind: "row", label: "Wi-Fi", value: connectionWords(model.network.connection), mark: "more", press: { t: "open", page: { p: "wifi" } } })
  if (model.identityManagement !== undefined) places.push({ id: "v:identity", kind: "row", label: "Identity and backup", mark: "more", press: { t: "open", page: { p: "identity" } } })
  if (places.length > 0) rows.push(head("v:head:places", "More"), ...places)
  rows.push(head("v:head:velvet", "Velvet"))
  rows.push({
    id: "v:motion", kind: "row", label: "Motion", value: state.still ? "Reduced" : "Full", mark: "step",
    note: "Reduced lands every change at once. Stays on this device.",
    press: { t: "motion", still: !state.still }, step: () => ({ t: "motion", still: !state.still }),
  })
  if (model.buildLabel !== undefined && model.buildLabel !== "") rows.push({ id: "v:build", kind: "row", label: "Build", value: model.buildLabel, note: model.buildLabel, rests: true })
  return { key: "page:settings", title: "Settings", rows, buttons: [], back: CLOSE }
}

function wifiPage(model: Model): PanelSpec {
  const network = model.network
  if (network === undefined) return { key: "page:wifi", title: "Wi-Fi", sub: "Korri cannot manage this device's network.", rows: [], buttons: [{ id: "b:back", label: "Back", press: CLOSE }], back: CLOSE }
  return { key: "page:wifi", title: "Wi-Fi", sub: connectionLine(network.connection), rows: wifiRows(model), buttons: [], back: CLOSE }
}

function storePage(model: Model): PanelSpec {
  const providers = model.store?.providers
  const rows: RowSpec[] = []
  let sub = "Places a plugin can search and get games from."
  if (providers === undefined) rows.push(note("p:none", "This device cannot get games."))
  else if (providers._tag === "Loading") rows.push(note("p:wait", "Looking for places to get games."))
  else if (providers._tag === "Problem") rows.push(problem("p:problem", providers.message), ...actionRows("p:a:", providers.actions))
  else if (providers.providers.length === 0) {
    sub = "No place to get games is turned on."
    rows.push(note("p:empty", "Turn on a plugin that provides games in Settings, then come back."), { id: "p:settings", kind: "row", label: "Open settings", mark: "more", press: { t: "open", page: { p: "settings" } } })
  } else for (const provider of providers.providers) rows.push({ id: `p:${provider.id}`, kind: "row", label: provider.title, mark: "more", press: { t: "open", page: { p: "provider", id: provider.id } } })
  const backRow: ButtonSpec[] = rows.some(row => row.press !== undefined) ? [] : [{ id: "b:back", label: "Back", press: CLOSE }]
  return { key: "page:store", title: "Find games", sub, rows, buttons: backRow, back: CLOSE }
}

export const STORE_QUERY_MAX = 40

function providerPage(id: string, model: Model): PanelSpec {
  const store = model.store
  const title = store?.providers._tag === "Ready" ? store.providers.providers.find(item => item.id === id)?.title ?? "Find games" : "Find games"
  const search = store !== undefined && store.search._tag !== "Idle" && store.search.providerId === id ? store.search : { _tag: "Idle" as const }
  const asked = search._tag === "Idle" ? "" : search.query
  const rows: RowSpec[] = [{
    id: "q:search", kind: "row", label: "Search", value: asked === "" ? "Type a name" : asked, mark: "more",
    press: { t: "type", entry: { e: "storeSearch", providerId: id, provider: title }, draft: asked },
  }]
  switch (search._tag) {
    case "Idle":
      rows.push(note("q:idle", "Search this place by a game's name."))
      break
    case "Searching":
      rows.push(note("q:wait", asked === "" ? "Searching." : `Searching for “${asked}”.`))
      break
    case "Problem":
      rows.push(problem("q:problem", search.message), ...actionRows("q:a:", search.actions))
      break
    case "Ready":
      if (search.entries.length === 0) rows.push(note("q:none", asked === "" ? "Nothing found." : `Nothing found for “${asked}”.`))
      for (const entry of search.entries) rows.push({ id: `q:e:${entry.id}`, kind: "row", label: entry.title, ...(entry.system === undefined ? {} : { value: entry.system }), mark: "more", press: seq(send({ _tag: "OpenStoreEntry", entryId: entry.id }), { t: "open", page: { p: "entry" } }) })
      break
  }
  return { key: `page:provider:${id}`, title, sub: "Find a game here and get it.", rows, buttons: [], back: CLOSE }
}

function getRows(get: SurfaceStoreGet): { rows: RowSpec[]; buttons: ButtonSpec[] } {
  switch (get._tag) {
    case "Offer":
      return { rows: get.note === undefined ? [] : [note("e:note", get.note)], buttons: actionButtons("e:a:", get.actions).map((button, index) => ({ ...button, quiet: index > 0 })) }
    case "Unavailable":
      return { rows: [problem("e:unavailable", get.reason)], buttons: [] }
    case "Confirm":
      return { rows: [note("e:kicker", get.kicker), ...(get.detail === undefined ? [] : [note("e:detail", get.detail)])], buttons: actionButtons("e:a:", get.actions) }
    case "Working": {
      const progress = get.progress
      const where = get.queued === true ? "Waiting for another download."
        : progress === undefined ? undefined
          : progress.total === undefined ? `${bytes(progress.done)} so far.` : `${bytes(progress.done)} of ${bytes(progress.total)}.`
      const rows: RowSpec[] = [note("e:kicker", get.kicker), ...(get.detail === undefined ? [] : [note("e:detail", get.detail)])]
      if (progress !== undefined && progress.total !== undefined && progress.total > 0) rows.push({ id: "e:bar", kind: "meter", label: where ?? "", share: Math.max(0, Math.min(1, progress.done / progress.total)) })
      else if (where !== undefined) rows.push(note("e:where", where))
      return { rows, buttons: actionButtons("e:a:", get.actions) }
    }
    case "InLibrary": {
      const rows: RowSpec[] = [note("e:in", get.detail ?? "This game is in your library.")]
      const buttons: ButtonSpec[] = get.gameId === undefined ? [] : [{
        id: "e:play", label: "Play", icon: "play",
        // The game is in the library: the store closes and the game starts, so the library is where the person comes back to.
        press: seq({ t: "home" }, send({ _tag: "LaunchGame", gameId: get.gameId })),
      }]
      return { rows, buttons }
    }
    case "Problem":
      return { rows: [problem("e:kicker", get.kicker), note("e:reason", get.reason)], buttons: actionButtons("e:a:", get.actions) }
  }
}

function entryPage(model: Model): PanelSpec {
  const details = model.store?.details
  const back: ButtonSpec = { id: "b:back", label: "Back", quiet: true, press: CLOSE }
  if (details === undefined || details._tag === "Closed") return { key: "page:entry", title: "Find games", rows: [note("e:closed", "No game is open.")], buttons: [back], back: CLOSE }
  const entry = details.entry
  const base = { key: "page:entry", title: entry.title, ...(entry.system === undefined ? {} : { sub: entry.system }), back: CLOSE }
  if (details._tag === "Loading") return { ...base, rows: [note("e:wait", "Reading about this game.")], buttons: [back] }
  if (details._tag === "Problem") return { ...base, rows: [problem("e:problem", details.message)], buttons: [...actionButtons("e:a:", details.actions), back] }
  const got = getRows(details.get)
  return { ...base, rows: [...(details.description === undefined ? [] : [note("e:about", details.description)]), ...got.rows], buttons: [...got.buttons, back], ...(got.buttons[0] === undefined ? {} : { first: got.buttons[0].id }) }
}

function invitesPage(model: Model): PanelSpec {
  const invites = model.invites ?? []
  const rows: RowSpec[] = []
  if (invites.length === 0) rows.push(note("i:none", "Nobody is asking you to play right now."))
  invites.forEach((invite, index) => {
    rows.push(head(`i:${index}`, `${invite.personName} plays ${invite.gameTitle}`), note(`i:${index}:on`, `On ${invite.deviceLabel}.`))
    rows.push(...actionRows(`i:${index}:`, invite.actions))
  })
  return { key: "page:invites", title: "Invites", sub: "Sessions on your other devices that this one can join.", rows, buttons: rows.some(row => row.press !== undefined) ? [] : [{ id: "b:back", label: "Back", press: CLOSE }], back: CLOSE }
}

/** A public key, short: its head and its tail. */
export const shortKey = (key: string): string => (key.length <= 20 ? key : `${key.slice(0, 10)}…${key.slice(-6)}`)

function identityPage(state: State, model: Model): PanelSpec {
  const identity = model.identityManagement
  const back: ButtonSpec = { id: "b:back", label: "Back", press: CLOSE }
  if (identity === undefined) return { key: "page:identity", title: "Identity and backup", sub: "This device cannot manage its identity.", rows: [], buttons: [back], back: CLOSE }
  const status = identity.status
  const busy = status._tag === "Working"
  // A finished backup is still shown: starting something else puts it away first.
  const start = (act: Act): Act => (busy ? NONE : status._tag === "BackupReady" ? seq(send({ _tag: "DismissIdentityStatus" }), act) : act)
  const rows: RowSpec[] = [
    identity.localBackupAvailable
      ? { id: "d:backup", kind: "row", label: "Back up this identity", note: "Makes a copy locked with a password you choose.", mark: "more", press: start({ t: "type", entry: { e: "backupPassword" } }) }
      : { id: "d:backup", kind: "row", label: "Back up this identity", note: "This identity's key is not on this device, so it cannot be backed up here.", dim: true, rests: true },
    { id: "d:switch", kind: "row", label: "Switch to a backup", note: "Brings an identity in from its backup and its password.", mark: "more", press: start({ t: "type", entry: { e: "switchSecret" } }) },
    { id: "d:bunker", kind: "row", label: "Switch to a bunker", note: "Signs with a remote signer, by its bunker address.", mark: "more", press: start({ t: "type", entry: { e: "bunker" }, draft: "bunker://" }) },
  ]
  if (identity.retiredPublicKeys.length > 0) rows.push(head("d:retired", "Retired identities"))
  for (const key of identity.retiredPublicKeys) {
    const remove: Act = { t: "ask", ask: question("Delete this retired identity?", [shortKey(key)], "Delete", { t: "ask", ask: question("Is it backed up?", ["Without a backup this identity is gone for good."], "Yes, delete it", send({ _tag: "DeleteRetiredIdentity", publicKey: key, backupConfirmed: true })) }) }
    rows.push({
      id: `d:r:${key}`, kind: "row", label: shortKey(key), mark: "more",
      press: start({
        t: "ask", ask: {
          title: "Retired identity", lines: [shortKey(key), "It was used on this device before."],
          buttons: [{ label: "Cancel", act: NONE }, { label: "Back it up", act: { t: "type", entry: { e: "backupPassword", retiredPublicKey: key } } }, { label: "Delete", danger: true, act: remove }],
        },
      }),
    })
  }
  const base = { key: "page:identity", title: "Identity and backup", sub: "Your identity is the key your devices trust each other by.", rows, buttons: [], back: status._tag === "BackupReady" ? seq(send({ _tag: "DismissIdentityStatus" }), { t: "identity", flow: { f: "idle" } }, CLOSE) : seq({ t: "identity", flow: { f: "idle" } }, CLOSE) }
  if (status._tag === "BackupReady") {
    return { ...base, side: { kind: "qr", title: "Your backup", lines: ["Scan it or write it down. With your password it brings this identity to another device.", status.encryptedSecret], qr: status.encryptedSecret } }
  }
  return base
}

export function pagePanel(page: PageRef, state: State, model: Model): PanelSpec {
  switch (page.p) {
    case "settings":
      return settingsPage(state, model)
    case "wifi":
      return wifiPage(model)
    case "store":
      return storePage(model)
    case "provider":
      return providerPage(page.id, model)
    case "entry":
      return entryPage(model)
    case "invites":
      return invitesPage(model)
    case "identity":
      return identityPage(state, model)
  }
}

// ---- Korri's own pages ----

/** The steps of first boot Korri gives reason for, in order. */
export function onboardingSteps(model: Model): readonly ("controllers" | "wifi" | "name" | "starter" | "finish")[] {
  return ["controllers", ...(model.network === undefined ? [] : ["wifi" as const]), "name", ...(model.onboarding?.starterPack === undefined ? [] : ["starter" as const]), "finish"]
}

const anyHeard = (model: Model): boolean => (model.onboarding?.controllers ?? []).some(controller => controller.heard)
const online = (model: Model): boolean => model.network === undefined || model.network.connection._tag !== "Offline"

function onboardingPanel(state: State, model: Model): PanelSpec {
  const steps = onboardingSteps(model)
  const at = Math.max(0, Math.min(state.onboarding.step, steps.length - 1))
  const step = steps[at]!
  const go = (to: number): Act => ({ t: "onboarding", step: to })
  const back: Act = at === 0 ? NONE : go(at - 1)
  const next = (enabled: boolean, label = "Next"): ButtonSpec => ({ id: "b:next", label, ...(enabled ? {} : { dim: true }), press: enabled ? go(at + 1) : NONE })
  const base = { key: `setup:${step}`, step: at + 1, steps: steps.length, back }
  switch (step) {
    case "controllers": {
      const controllers = model.onboarding?.controllers ?? []
      const rows: RowSpec[] = controllers.length === 0
        ? [note("c:none", "No controller is connected yet.")]
        : controllers.map(controller => ({ id: `c:${controller.id}`, kind: "row" as const, label: controller.name, value: controller.heard ? "Heard" : "Waiting", ...(controller.heard ? { mark: "check" as const } : {}), rests: true }))
      return { ...base, title: "Controllers", sub: "Press a button on each controller you will play with.", rows, buttons: [next(anyHeard(model))], first: "b:next" }
    }
    case "wifi": {
      const rows = wifiRows(model)
      return {
        ...base, title: "Wi-Fi", sub: model.network === undefined ? "" : connectionLine(model.network.connection), rows,
        buttons: [next(online(model)), { id: "b:skip", label: "Skip for now", quiet: true, press: go(at + 1) }],
        ...(online(model) ? { first: "b:next" } : {}),
      }
    }
    case "name": {
      const items = model.settings.flatMap(group => group.items).filter(item => item.id === "device-name" || item.id === "person-name")
      const rows = items.flatMap(item => settingRows(item, model.settingsStatus, state))
      return { ...base, title: "Names", sub: "What this device, and you, are called on your other devices.", rows: rows.length === 0 ? [note("n:none", "Korri has named this device already.")] : rows, buttons: [next(true)], ...(rows.length === 0 ? { first: "b:next" } : {}) }
    }
    case "starter": {
      const pack = model.onboarding?.starterPack
      const rows: RowSpec[] = []
      let buttons: ButtonSpec[] = [next(true)]
      if (pack?._tag === "Offered") {
        rows.push(note("k:offer", "A few free games and what runs them, ready to play."))
        buttons = [...actionButtons("k:a:", pack.actions).map((button, index) => ({ ...button, quiet: index > 0 })), { ...next(true, "Skip for now"), quiet: true }]
      } else if (pack?._tag === "Installing") {
        rows.push(note("k:installing", "Getting the Starter pack. You can go on; it finishes by itself."))
      } else if (pack?._tag === "Installed") rows.push(note("k:installed", "The Starter pack is in your library."))
      else if (pack?._tag === "Failed") {
        rows.push(problem("k:failed", pack.message))
        buttons = [...actionButtons("k:a:", pack.actions), next(true)]
      }
      return { ...base, title: "Starter pack", rows, buttons, first: buttons[0]!.id }
    }
    case "finish":
      return {
        ...base, title: "The house is ready", sub: "This device starts with an identity of its own. You can bring another one in later, under Identity and backup.", rows: [],
        buttons: [{ id: "b:finish", label: state.onboarding.finishing ? "Finishing" : "Finish", ...(state.onboarding.finishing ? { dim: true } : {}), press: state.onboarding.finishing ? NONE : seq({ t: "onboarding", step: at, finishing: true }, send({ _tag: "FinishOnboarding" })) }],
        first: "b:finish",
      }
  }
}

function runnerPanel(model: Model): PanelSpec {
  const choice = model.runnerChoice
  if (choice === undefined || choice._tag === "Closed") return { key: "korri:runner", title: "Runner", rows: [], buttons: [], back: NONE }
  const rows: RowSpec[] = []
  choice.warnings.forEach((warning, index) => rows.push(problem(`r:w:${index}`, warning)))
  if (choice._tag === "Loading" || choice._tag === "Busy") rows.push(note("r:state", choice._tag === "Loading" ? "Reading what can run this game." : "Working."))
  for (const saved of choice.saved.filter(saved => !choice.routes.some(route => route.runnerId === saved.runnerId))) {
    rows.push({ id: `r:saved:${saved.label}`, kind: "row", label: saved.label, value: saved.runnerId, note: "This saved choice names a runner that is not offered now.", dim: true, rests: true })
  }
  for (const route of choice.routes) {
    const saved = choice.saved.filter(item => item.runnerId === route.runnerId).map(item => item.label)
    rows.push({ id: `r:${route.runnerId}`, kind: "row", label: route.runnerId, value: saved.join(", "), ...(saved.length > 0 ? { mark: "check" as const } : {}), note: `${route.program} ${route.runnerBuild}`, rests: true })
    route.warnings.forEach((warning, index) => rows.push(problem(`r:${route.runnerId}:w:${index}`, warning)))
    rows.push(...actionRows(`r:${route.runnerId}:`, route.actions))
  }
  if (choice.actions.length > 0) rows.push(head("r:all", "This game"), ...actionRows("r:a:", choice.actions))
  // Back is Korri's here: it publishes its own way out as an action, and Velvet never guesses which one that is.
  return { key: "korri:runner", title: choice.gameTitle, sub: choice.message, rows, buttons: [], back: NONE }
}

/**
 * Korri's launch settings (ADR 0066): one list. Each setting stands once,
 * under its section, with the value the next launch would use, the same for
 * a game this device runs and a game another device runs. A row Korri gives
 * no way to change rests and says why. A changed row says what it was, beside
 * its value. While
 * a row is changed, the last row is where the changes are kept (this launch,
 * this game, every game...). Profiles are rows of Korri's own Profile group.
 * The rows show while Korri is ready, and beside an error: the rows of this
 * device still work then.
 */
function launchPanel(state: State, model: Model): PanelSpec {
  const launch = model.launchSettings
  if (launch === undefined || launch._tag === "Closed") return { key: "korri:launch", title: "Launch settings", rows: [], buttons: [], back: NONE }
  const close = launch.actions.find(action => action.id.endsWith(":cancel"))
  const rows: RowSpec[] = []
  if (launch._tag === "Conflict" || launch._tag === "Error") rows.push(problem("l:message", launch.message))
  if (launch._tag === "Loading") rows.push(note("l:wait", "Reading this game's launch settings."))
  launch.notes.forEach((line, index) => rows.push(note(`l:note:${index}`, line)))
  const listed = launch._tag === "Ready" || launch._tag === "Error"
  const asks = launch._tag === "Conflict" || launch._tag === "Error"
  const entry = launch.actions.find(action => action.enabled)
  if (listed) {
    const was = new Map(launch.changes.map(change => [change.id, change.was] as const))
    launch.groups.filter(group => group.items.length > 0).forEach((group, index) => {
      rows.push(head(`l:head:${index}`, group.title))
      for (const item of group.items) {
        const earlier = was.get(item.id)
        for (const row of settingRows(item, model.settingsStatus, state, "l:")) {
          // A row changed and not kept yet says what it was, beside its value: seen without the focus on it.
          rows.push(earlier === undefined || row.id !== `l:${item.id}` ? row : { ...row, value: row.value === undefined || row.value === "" ? `Was ${earlier}` : `${row.value} (was ${earlier})` })
        }
      }
    })
    // Ready with no row is no fault: Korri has nothing to set for this game.
    if (launch._tag === "Ready" && rows.length === launch.notes.length) rows.push(note("l:none", "No launch settings for this game."))
    // Where the changes are kept: there only while a row is changed.
    if (launch.keep !== undefined && was.size > 0) {
      rows.push(head("l:head:keep", "Changes"))
      rows.push(...settingRows(launch.keep, model.settingsStatus, state, "l:"))
    }
  }
  return {
    key: "korri:launch", title: "Launch settings",
    sub: [launch.gameTitle, launch.runsOn].filter(part => part !== undefined && part !== "").join(" · "),
    rows, buttons: actionButtons("l:a:", launch.actions),
    // Korri's message asks for an action: a conflict or an error opens on the first one that can be pressed.
    ...(asks && entry !== undefined ? { first: `l:a:${entry.id}` } : {}),
    // The editor is Korri's to close: Back runs its close action.
    back: close === undefined ? NONE : run(close.id),
  }
}

function savesPanel(state: State, model: Model): PanelSpec {
  const history = model.saveHistory
  if (history === undefined) return { key: "korri:saves", title: "Saves", rows: [], buttons: [], back: NONE }
  const game = gameById(state.lists, model, history.gameId) ?? (state.detail?.game.id === history.gameId ? state.detail.game : undefined)
  const rows: RowSpec[] = []
  if (history._tag === "Loading") rows.push(note("h:wait", "Reading this game's saves."))
  else if (history._tag === "Error") rows.push(problem("h:error", history.message))
  else if (history.entries.length === 0) rows.push(note("h:none", "No saves yet for this game."))
  else {
    for (const entry of history.entries) {
      const facts = `${duration(entry.durationSeconds)} session${entry.interrupted ? ", cut short" : ""}.`
      const base = { id: `h:${entry.snapshotId}`, kind: "row" as const, label: saved(entry.endedAt, model.publishedAt), value: entry.device }
      if (entry.current || entry.restore === undefined) rows.push({ ...base, note: entry.current ? `${facts} This device continues from it.` : facts, ...(entry.current ? { mark: "check" as const } : {}), rests: true })
      else {
        const restore = entry.restore
        rows.push({
          ...base, note: `${facts} ${restore.description ?? restore.label}`, ...(restore.enabled ? {} : { dim: true }),
          ...(restore.enabled
            ? { press: { t: "ask", ask: question(restore.label, [`${saved(entry.endedAt, model.publishedAt)}, ${entry.device}.`, restore.description ?? "This device will continue from this save."], restore.label, run(restore.id)) } as Act }
            : { rests: true }),
        })
      }
    }
  }
  // Korri publishes its one way out as an action: with exactly one, Back takes it.
  return { key: "korri:saves", title: "Saves", ...(game === undefined ? {} : { sub: game.title }), rows, buttons: actionButtons("h:a:", history.actions), back: history.actions.length === 1 && history.actions[0]!.enabled ? run(history.actions[0]!.id) : NONE }
}

// ---- The gameplay overlay ----

function controlRows(control: SurfaceGameplayControl, state: State): RowSpec {
  const id = `c:${control.id}`
  const base = { id, kind: "row" as const, label: control.label, ...(control.description === undefined ? {} : { note: control.description }) }
  if (!control.enabled) return { ...base, dim: true, rests: true, ...(control.disabledReason === undefined ? {} : { note: control.disabledReason }) }
  const invoke = (value?: GameplayValue): Act =>
    seq(send(value === undefined ? { _tag: "InvokeGameplayControl", controlId: control.id } : { _tag: "InvokeGameplayControl", controlId: control.id, value }), ...(control.dismissOnSuccess ? [{ t: "leaving" } as const] : []))
  const guard = (act: Act): Act => (control.destructive ? { t: "ask", ask: question(control.label, control.description === undefined ? [] : [control.description], control.label, act) } : act)
  const interaction = control.interaction
  switch (interaction.kind) {
    case "command":
      return { ...base, ...(control.destructive ? { danger: true } : {}), press: guard(invoke()) }
    case "toggle": {
      const flip = guard(invoke({ kind: "toggle", value: !interaction.value }))
      return { ...base, value: interaction.value ? interaction.trueLabel : interaction.falseLabel, mark: interaction.value ? "on" : "off", press: flip, step: () => flip }
    }
    case "choice": {
      const count = interaction.options.length
      const at = interaction.options.findIndex(option => option.value === interaction.value)
      const to = (direction: -1 | 1): Act => {
        const next = interaction.options[at < 0 ? (direction > 0 ? 0 : count - 1) : (at + direction + count) % count]
        return next === undefined || next.value === interaction.value ? NONE : guard(invoke({ kind: "choice", value: next.value }))
      }
      return { ...base, value: interaction.options[at]?.label ?? "", mark: "step", press: to(1), step: to }
    }
    case "range": {
      const shown = state.overlay.moving[control.id] ?? interaction.value
      const span = interaction.max - interaction.min
      const snap = (value: number): number => {
        const step = interaction.step > 0 ? interaction.step : 1
        return Number(Math.min(interaction.max, Math.max(interaction.min, interaction.min + Math.round((value - interaction.min) / step) * step)).toFixed(6))
      }
      const set = (value: number): Act => (value === shown ? NONE : seq({ t: "moving", controlId: control.id, value }, invoke({ kind: "range", value })))
      return {
        ...base, kind: "range", value: String(shown), share: span <= 0 ? 0 : Math.max(0, Math.min(1, (shown - interaction.min) / span)),
        step: direction => set(snap(shown + direction * (interaction.step > 0 ? interaction.step : 1))),
        slide: at => set(snap(interaction.min + at * span)),
        rests: true,
      }
    }
  }
}

export function overlayPanel(overlay: SurfaceGameplayOverlayPresentation, state: State): PanelSpec {
  const rows: RowSpec[] = overlay.controls.map(control => controlRows(control, state))
  for (const group of overlay.groups) rows.push(head(`c:head:${group.id}`, group.label), ...group.controls.map(control => controlRows(control, state)))
  const leave = send({ _tag: "DismissGameplayOverlay" })
  return { key: "overlay", title: overlay.title ?? "Paused", rows, buttons: [], back: leave, menu: leave }
}

// ---- Which panel holds the screen ----

export type Shown = "overlay" | "setup" | "runner" | "launch" | "saves" | "page" | "detail" | "set" | "home"

export const runnerOpen = (model: Model): boolean => model.runnerChoice !== undefined && model.runnerChoice._tag !== "Closed"
export const launchOpen = (model: Model): boolean => model.launchSettings !== undefined && model.launchSettings._tag !== "Closed"

/** What takes the screen, first match wins. Sheets, the keyboard and the curtain stand over it. */
export function shown(state: State, model: Model): Shown {
  if (model.presentation.kind === "gameplay-overlay") return "overlay"
  if (model.onboarding !== undefined) return "setup"
  if (runnerOpen(model)) return "runner"
  if (launchOpen(model)) return "launch"
  if (model.saveHistory !== undefined) return "saves"
  if (state.pages.length > 0) return "page"
  if (state.detail !== undefined) return "detail"
  if (state.set !== undefined) return "set"
  return "home"
}

let lastPanel: { state: State; model: Model; panel: PanelSpec | undefined } | undefined

/** The panel that holds the screen, if a panel does. */
export function screenPanel(state: State, model: Model): PanelSpec | undefined {
  if (lastPanel !== undefined && lastPanel.state === state && lastPanel.model === model) return lastPanel.panel
  let panel: PanelSpec | undefined
  switch (shown(state, model)) {
    case "overlay":
      panel = model.presentation.kind === "gameplay-overlay" ? overlayPanel(model.presentation, state) : undefined
      break
    case "setup":
      panel = onboardingPanel(state, model)
      break
    case "runner":
      panel = runnerPanel(model)
      break
    case "launch":
      panel = launchPanel(state, model)
      break
    case "saves":
      panel = savesPanel(state, model)
      break
    case "page":
      panel = pagePanel(state.pages[state.pages.length - 1]!, state, model)
      break
    default:
      panel = undefined
  }
  lastPanel = { state, model, panel }
  return panel
}

// ---- The keyboard: what an entry is called, and what Done does with the draft ----

export interface EntryHeading {
  readonly title: string
  /** A name Korri publishes (an SSID, a provider). */
  readonly name?: string
  readonly secret: boolean
  readonly maxLength?: number
  readonly clearLabel?: string
  readonly placeholder?: string
  readonly note?: string
}

export function entryHeading(entry: Entry): EntryHeading {
  switch (entry.e) {
    case "setting":
      return {
        title: entry.label, secret: entry.secret,
        ...(entry.maxLength === undefined ? {} : { maxLength: entry.maxLength }),
        ...(entry.clearLabel === undefined ? {} : { clearLabel: entry.clearLabel }),
        ...(entry.placeholder === undefined ? {} : { placeholder: entry.placeholder }),
        ...(entry.secret ? { note: "Korri keeps it and never shows it again." } : {}),
      }
    case "wifiPassword":
      return { title: "Password", name: entry.ssid, secret: true, maxLength: WIFI_PASSWORD_MAX }
    case "hiddenSsid":
      return { title: "Network name", secret: false, maxLength: WIFI_SSID_MAX, note: "The name of a network that does not announce itself." }
    case "hiddenPassword":
      return { title: "Password", name: entry.ssid, secret: true, maxLength: WIFI_PASSWORD_MAX, note: "Leave it empty for an open network." }
    case "storeSearch":
      return { title: "Search", name: entry.provider, secret: false, maxLength: STORE_QUERY_MAX, placeholder: "A game's name" }
    case "backupPassword":
      return { title: "Password for the backup", secret: true, maxLength: 128, note: "You need this password to use the backup. Korri cannot recover it." }
    case "switchSecret":
      return { title: "The backup", secret: false, maxLength: 512, placeholder: "ncryptsec1…", note: "The long text of the backup, as it was shown." }
    case "switchPassword":
      return { title: "The backup's password", secret: true, maxLength: 128 }
    case "bunker":
      return { title: "Bunker address", secret: false, maxLength: 512 }
  }
}

/** Done: what the draft is for decides what follows. Undefined: the keyboard stays (nothing valid to send). */
export function commitEntry(entry: Entry, draft: string): Act | undefined {
  switch (entry.e) {
    case "setting": {
      // An empty secret says nothing: clearing one is its own key.
      if (entry.secret && draft === "") return NONE
      if (entry.numeric !== undefined) {
        if (draft.trim() === "") return entry.numeric.clearable ? send({ _tag: "ChangeSetting", settingId: entry.settingId, value: "" }) : NONE
        if (!numberFits(draft, entry.numeric)) return undefined
      }
      if (!entry.secret && entry.numeric === undefined && draft === "" && entry.allowEmpty !== true) return NONE
      return send({ _tag: "ChangeSetting", settingId: entry.settingId, value: draft })
    }
    case "wifiPassword":
      return draft === "" ? undefined : send({ _tag: "ConnectWifi", networkId: entry.networkId, password: draft })
    case "hiddenSsid": {
      const ssid = draft.trim()
      return ssid === "" ? undefined : { t: "type", entry: { e: "hiddenPassword", ssid } }
    }
    case "hiddenPassword":
      return send(draft === "" ? { _tag: "ConnectHiddenWifi", ssid: entry.ssid } : { _tag: "ConnectHiddenWifi", ssid: entry.ssid, password: draft })
    case "storeSearch":
      return send({ _tag: "SearchStore", providerId: entry.providerId, query: draft.trim() })
    case "backupPassword":
      return draft === "" ? undefined : seq(send({ _tag: "ExportIdentityBackup", password: draft, ...(entry.retiredPublicKey === undefined ? {} : { retiredPublicKey: entry.retiredPublicKey }) }), { t: "identity", flow: { f: "backup" } })
    case "switchSecret": {
      // A backup's secret is bech32: whitespace in it is a typing slip, never part of it.
      const secret = draft.replace(/\s+/g, "")
      return secret === "" ? undefined : { t: "type", entry: { e: "switchPassword", encryptedSecret: secret } }
    }
    case "switchPassword":
      return draft === "" ? undefined : { t: "identity", flow: { f: "disposition", source: { kind: "backup", encryptedSecret: entry.encryptedSecret, password: draft } } }
    case "bunker": {
      const uri = draft.trim()
      return uri === "" || uri === "bunker://" ? undefined : { t: "identity", flow: { f: "disposition", source: { kind: "bunker", bunkerUri: uri } } }
    }
  }
}

/** The clear key: Korri's clearLabel on a secret or a number it lets clear. */
export function clearEntry(entry: Entry): Act | undefined {
  return entry.e === "setting" && entry.clearLabel !== undefined ? send({ _tag: "ChangeSetting", settingId: entry.settingId, value: "" }) : undefined
}

export { sameQuery }
