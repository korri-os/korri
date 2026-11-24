/**
 * Installs (ADR 0092): a game that only another device holds put on this
 * one, and the list of every install this device runs.
 *
 *   the detail  beside Play (stream play, where the game is), "Install and
 *               play"; "Install for later" in the game's More. Korri never
 *               picks either: Play keeps the first place and the focus, as
 *               it always had. Why an install cannot happen stands under
 *               the buttons, and the choice stays, dimmed, when pressing
 *               it again cannot help (no runner, not allowed). While the
 *               game installs: a bar under the buttons and "Installs".
 *   the curtain Korri's question when an install that ends in play takes
 *               longer than two minutes: wait for it, or stream now while
 *               it installs. Velvet's own curtain, because Korri leaves the
 *               asking to the surface (`streamMeanwhile`). The way out
 *               (wait) comes first and holds the focus, as every Velvet
 *               question does; Back is the same answer.
 *   the page    "Installs": the one that runs, its bar, the ones that wait
 *               in the person's order, the failed ones with their reason.
 *               Confirm on one opens its sheet: Install next, Move up, Move
 *               down, Cancel install; a failed one: Try again, Throw away.
 *               The sheet stays open on a move, so a press after a press
 *               moves it further.
 *
 * Reached from the house menu, Settings › More and the detail of a game
 * that installs. Every word is Velvet's (src/words.ts) except a failure's
 * reason, which Korri writes.
 */
import type { SurfaceInstall, SurfaceModel as Model } from "../../../../../contracts/generated/surface"
import type { CurtainSpec } from "./curtain"
import { whereOf } from "./art"
import { gameById } from "./lists"
import { NONE, send, seq, type Act, type Game, type PanelSpec, type RowSpec, type State } from "./types"
import { blocked, bytes, CHECKING, INSTALL_AND_PLAY, INSTALL_FOR_LATER, installed, INSTALLS, inLine, takes } from "./words"

const CLOSE: Act = { t: "close" }

/** The installs page, as an act. */
export const OPEN_INSTALLS: Act = { t: "open", page: { p: "installs" } }

export const installById = (model: Model, id: string): SurfaceInstall | undefined => (model.installs ?? []).find(install => install.id === id)

/**
 * Where one install that waits stands in the line: 1 is next. The one that
 * runs is not counted; nor are the failed ones, which stand last.
 */
export function placeOf(installs: readonly SurfaceInstall[], index: number): number {
  let place = 0
  for (let at = 0; at <= index; at++) if (installs[at]?.state._tag === "Waiting") place++
  return place
}

/** The words for where an install stands, for a row's value. */
function standing(installs: readonly SurfaceInstall[], index: number): string {
  const install = installs[index]!
  switch (install.state._tag) {
    case "Installing":
      return "Installing"
    case "Waiting":
      return inLine(placeOf(installs, index))
    case "Failed":
      return "Failed"
  }
}

// ---- The detail ----

export interface HereButton {
  readonly id: string
  readonly label: string
  readonly icon: string
  readonly quiet: boolean
  readonly dim?: boolean
  readonly act: Act
}

/** The detail's install choices, after Play. */
export function hereButtons(game: Game, model: Model): readonly HereButton[] {
  const here = game.installHere
  if (here === undefined) return []
  const play: HereButton = { id: "d:here", label: INSTALL_AND_PLAY, icon: "down", quiet: true, act: send({ _tag: "InstallGame", gameId: game.id, play: true }) }
  switch (here._tag) {
    case "Offered":
    case "NeedsPlugin":
      return [play]
    case "Checking":
      return [{ ...play, label: CHECKING, dim: true, act: NONE }]
    case "Installing": {
      const install = installById(model, here.installId)
      const list: HereButton = { id: "d:installs", label: INSTALLS, icon: "menu", quiet: true, act: OPEN_INSTALLS }
      // Installing for later: Install and play moves it to the front and starts it when it is done.
      return install?.playWhenReady === true ? [list] : [play, list]
    }
    case "Unavailable":
      // Pressing again checks again: worth it when space can be freed or a device woken.
      return here.reason === "notEnoughSpace" || here.reason === "noSource" ? [play] : [{ ...play, dim: true, act: NONE }]
  }
}

/** Whether the game's More holds "Install for later". */
export function installsLater(game: Game): boolean {
  const here = game.installHere
  return here !== undefined && (here._tag === "Offered" || here._tag === "NeedsPlugin" || (here._tag === "Unavailable" && (here.reason === "notEnoughSpace" || here.reason === "noSource")))
}

/** "Install for later" as a row of the game's More. */
export function laterRow(game: Game): RowSpec {
  return { id: "g:install-later", kind: "row", label: INSTALL_FOR_LATER, note: "Puts it on this device to play when you like. Play still streams it now.", press: seq(send({ _tag: "InstallGame", gameId: game.id, play: false }), CLOSE) }
}

/** The line under the detail's buttons, when the game can be installed here. Undefined: not an install's to say. */
export function hereNote(game: Game, model: Model): string | undefined {
  const here = game.installHere
  if (here === undefined) return undefined
  switch (here._tag) {
    case "Offered":
      return ""
    case "NeedsPlugin":
      return `To run it, this device needs the ${here.pluginTitle} plugin. Installing asks to add the plugin first.`
    case "Checking":
      return "Asking the devices that hold it how large it is."
    case "Unavailable":
      return blocked(here.reason, here.bytes)
    case "Installing": {
      const install = installById(model, here.installId)
      const then = install?.playWhenReady === true ? " It starts when it is installed." : ""
      if (install === undefined) return `Installing on this device.${then}`
      const installs = model.installs ?? []
      switch (install.state._tag) {
        case "Installing":
          return `Installing on this device: ${installed(install.state.progress.done, install.state.progress.total, install.state.secondsLeft)}.${then}`
        case "Waiting":
          return `Waiting to install: ${inLine(placeOf(installs, installs.indexOf(install))).toLowerCase()}.${then}`
        case "Failed":
          return install.state.reason
      }
    }
  }
}

/** The bar under the detail's buttons: how far the game's install has got, 0 to 1; undefined: no bar. */
export function hereShare(game: Game, model: Model): number | undefined {
  const here = game.installHere
  if (here?._tag !== "Installing") return undefined
  const state = installById(model, here.installId)?.state
  if (state?._tag !== "Installing") return undefined
  const total = state.progress.total
  return total === undefined || total <= 0 ? undefined : Math.max(0, Math.min(1, state.progress.done / total))
}

// ---- The page ----

export function installsPage(model: Model): PanelSpec {
  const installs = model.installs ?? []
  const rows: RowSpec[] = []
  installs.forEach((install, index) => {
    const state = install.state
    const note = state._tag === "Installing" ? (install.playWhenReady === true ? "Starts when it is installed." : "")
      : state._tag === "Waiting" ? (state.bytes === undefined ? "" : bytes(state.bytes))
        : ""
    rows.push({
      id: `n:${install.id}`, kind: "row", label: install.title, value: standing(installs, index), mark: "more",
      ...(note === "" ? {} : { note }),
      press: { t: "sheet", sheet: "install", installId: install.id },
    })
    if (state._tag === "Installing") {
      const words = installed(state.progress.done, state.progress.total, state.secondsLeft)
      const total = state.progress.total
      rows.push(total === undefined || total <= 0
        ? { id: `n:${install.id}:at`, kind: "note", label: words }
        : { id: `n:${install.id}:at`, kind: "meter", label: words, share: Math.max(0, Math.min(1, state.progress.done / total)) })
    }
    if (state._tag === "Failed") rows.push({ id: `n:${install.id}:why`, kind: "problem", label: state.reason })
  })
  if (installs.length === 0) rows.push({ id: "n:none", kind: "note", label: "Nothing is installing. A game another device holds can be installed from its page." })
  return {
    key: "page:installs", title: INSTALLS, sub: "One install runs at a time, the top one first.", rows,
    buttons: installs.length === 0 ? [{ id: "b:back", label: "Back", press: CLOSE }] : [], back: CLOSE,
  }
}

/** One install's sheet: what can be done with it, by where it stands. */
export function installSheet(id: string | undefined, model: Model): PanelSpec {
  const installs = model.installs ?? []
  const index = installs.findIndex(install => install.id === id)
  const install = installs[index]
  if (install === undefined) return { key: "sheet:install", title: INSTALLS, rows: [{ id: "i:gone", kind: "note", label: "This install is no longer in the list." }], buttons: [], back: CLOSE }
  const move = (to: number): Act => send({ _tag: "MoveInstall", installId: install.id, index: to })
  const cancel = (label: string, line: string): RowSpec => ({
    id: "i:cancel", kind: "row", label, danger: true,
    // The way out says what it keeps: "Cancel" beside "Cancel install" would read as either.
    press: { t: "ask", ask: { title: label, lines: [install.title, line], buttons: [{ label: "Keep it", act: NONE }, { label, danger: true, act: seq(send({ _tag: "CancelInstall", installId: install.id }), CLOSE) }] } },
  })
  const rows: RowSpec[] = []
  const state = install.state
  switch (state._tag) {
    case "Installing":
      rows.push({ id: "i:at", kind: "note", label: `Installing: ${installed(state.progress.done, state.progress.total, state.secondsLeft)}.` })
      rows.push(cancel("Cancel install", "What is installed so far is deleted."))
      break
    case "Waiting": {
      const place = placeOf(installs, index)
      const below = installs[index + 1]?.state._tag === "Waiting"
      rows.push({ id: "i:at", kind: "note", label: `${inLine(place)}.${state.bytes === undefined ? "" : ` ${bytes(state.bytes)}.`}` })
      // Every row stands in every state, so a place in the sheet is always the same row; one that cannot apply rests, dimmed.
      rows.push(place > 1 ? { id: "i:next", kind: "row", label: "Install next", press: move(0) } : { id: "i:next", kind: "row", label: "Install next", note: "It is next already.", dim: true, rests: true })
      rows.push(place > 1 ? { id: "i:up", kind: "row", label: "Move up", press: move(index - 1) } : { id: "i:up", kind: "row", label: "Move up", note: "It is next already.", dim: true, rests: true })
      rows.push(below ? { id: "i:down", kind: "row", label: "Move down", press: move(index + 1) } : { id: "i:down", kind: "row", label: "Move down", note: "It is last in line.", dim: true, rests: true })
      rows.push(cancel("Cancel install", "It leaves the line."))
      break
    }
    case "Failed":
      rows.push({ id: "i:why", kind: "problem", label: state.reason })
      if (install.gameId !== undefined) rows.push({ id: "i:again", kind: "row", label: "Try again", note: "Goes on from what was installed.", press: seq(send({ _tag: "InstallGame", gameId: install.gameId, play: false }), CLOSE) })
      rows.push(cancel("Throw away", "What was installed is deleted."))
      break
  }
  return { key: "sheet:install", title: install.title, rows, buttons: [], back: CLOSE }
}

// ---- Stream meanwhile ----

/**
 * Korri's question when play here takes longer than two minutes: the
 * install goes on either way; the answer says whether the game streams now
 * (and then does not start by itself when installed) or starts here when
 * the install is done.
 */
export function meanwhileCurtain(state: State, model: Model): CurtainSpec | undefined {
  if (model.presentation.kind !== "catalog") return undefined
  const install = (model.installs ?? []).find(candidate => candidate.streamMeanwhile === true)
  if (install === undefined) return undefined
  const game = install.gameId === undefined ? undefined : gameById(state.lists, model, install.gameId) ?? (state.detail?.game.id === install.gameId ? state.detail.game : undefined)
  const answer = (stream: boolean, launchLocationId?: string): Act =>
    send({ _tag: "AnswerStreamMeanwhile", installId: install.id, stream, ...(launchLocationId === undefined ? {} : { launchLocationId }) })
  const places = game?.launchLocations ?? []
  const wait = { label: "Wait for the install", act: answer(false) }
  // Several devices hold it: one way to stream a device, none chosen for the person.
  const streams = places.length > 1 ? places.map(place => ({ label: `Stream from ${place.label}`, act: answer(true, place.id) })) : [{ label: "Stream while it installs", act: answer(true) }]
  const where = game === undefined ? undefined : whereOf(game)
  const from = where === undefined || where === "" || places.length > 1 ? "" : ` from ${where}`
  const secondsLeft = install.state._tag === "Installing" ? install.state.secondsLeft : undefined
  return {
    key: `meanwhile:${install.id}`, kind: "ask", kicker: takes(secondsLeft), name: install.title, ...(game === undefined ? {} : { game }),
    words: [`Play it now by streaming it${from}, or wait and it starts here when it is installed. It keeps installing either way.`],
    progress: { kind: "none" }, buttons: [wait, ...streams], first: 0, back: answer(false),
  }
}
