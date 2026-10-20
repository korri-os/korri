/**
 * Zone as a whole: every model, which page is shown, and the one function
 * that routes a message to the model it belongs to.
 *
 *     nav         where the player is: the tab, the GAMES cursor, Zone's own
 *                 layers (navigation.ts)
 *     options     the OPTIONS page's cursor, keyboard and dialog (options.ts)
 *     runWith     the RUN WITH page's cursor (run-with.ts)
 *     identity    SAVE DATA's card and identity flow (identity.ts)
 *     status      the status layer over every page (status.ts)
 *     overlay     PAUSED, while Korri presents gameplay (overlay.ts)
 *     onboarding  first boot, while Korri asks for it (onboarding.ts)
 *     look        the look this device shows Zone in, Zone's own (look.ts);
 *                 OPTIONS' LOOK row steps it
 *
 * The models stay pure and separate; this file only composes them. A page
 * reports in its own model's words (`Options`, `SaveData`, …) and the
 * message goes to that model's update unchanged. Korri publishing goes to
 * every model. The host's four buttons are routed here, by what is shown:
 *
 *     back     the status layer's Problem first; Busy is never cancelled
 *              by it and Confirm is never answered by it. Then the shown page's own rule; on OPTIONS and SAVE
 *              DATA, when the page has nothing to close, to the GAMES tab;
 *              on GAMES, navigation's rule; then nothing.
 *     menu     the cursor to the tabs. A keyboard on OPTIONS commits its
 *              draft instead (OPTIONS' rule); a dialog or an identity step
 *              holds the cursor, so menu does nothing there.
 *     options  GAME OPTS. on GAMES; nothing anywhere else.
 *     system   OPTIONS with the cursor on its SYSTEM group.
 *
 * PAUSED, first boot and RUN WITH take the whole screen and the tabs are not
 * on it, so menu, options and system do nothing while one is shown.
 */
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { ZONE_DEFAULT_LOOK, type ZoneLook } from "../plan/look"
import { zoneNetworkRowId, ZONE_SYSTEM_LOOK, type ZoneOptionsSlices } from "../plan/options"
import {
  IDENTITY_START,
  update as updateIdentity,
  type IdentityMessage,
  type IdentityModel,
} from "./identity"
import { updateLook } from "./look"
import type { ZoneHostButton } from "./messages"
import { update as updateNav, ZONE_START, type ZoneNavigation, type ZoneNavMessage } from "./navigation"
import {
  updateOnboarding,
  ZONE_ONBOARDING_START,
  type ZoneOnboardingMessage,
  type ZoneOnboardingModel,
} from "./onboarding"
import {
  focusSystemGroup,
  optionsLayer,
  optionsStart,
  update as updateOptions,
  type ZoneOptions,
  type ZoneOptionsMessage,
} from "./options"
import { updateOverlay, ZONE_OVERLAY_START, type ZoneOverlayMessage, type ZoneOverlayModel } from "./overlay"
import type { ZoneRequest, ZoneStep } from "./requests"
import { RUN_WITH_START, update as updateRunWith, type ZoneRunWith, type ZoneRunWithMessage } from "./run-with"
import { updateStatus, ZONE_STATUS_START, type ZoneStatusMessage, type ZoneStatusModel } from "./status"

export interface ZoneSurfaceState {
  readonly nav: ZoneNavigation
  readonly options: ZoneOptions
  readonly runWith: ZoneRunWith
  readonly identity: IdentityModel
  readonly status: ZoneStatusModel
  readonly overlay: ZoneOverlayModel
  readonly onboarding: ZoneOnboardingModel
  readonly look: ZoneLook
}

/** What OPTIONS reads: Korri's slices and Zone's own look, for its LOOK row. */
export const withLook = (state: ZoneSurfaceState, korri: SurfaceModel): ZoneOptionsSlices => ({ ...korri, look: state.look })

export type ZoneSurfaceMessage =
  | ZoneHostButton
  /** Korri published a new model: every model is settled against it. */
  | { readonly _tag: "KorriPublished" }
  | { readonly _tag: "Games"; readonly message: ZoneNavMessage }
  | { readonly _tag: "Options"; readonly message: ZoneOptionsMessage }
  | { readonly _tag: "RunWith"; readonly message: ZoneRunWithMessage }
  | { readonly _tag: "SaveData"; readonly message: IdentityMessage }
  | { readonly _tag: "Status"; readonly message: ZoneStatusMessage }
  | { readonly _tag: "Paused"; readonly message: ZoneOverlayMessage }
  | { readonly _tag: "Onboarding"; readonly message: ZoneOnboardingMessage }

/**
 * What takes the screen, first match wins:
 *
 *     paused      Korri presents the gameplay overlay
 *     join        first boot handed a secured network to OPTIONS' join
 *                 flow, and its keyboard is up
 *     onboarding  Korri asks for first boot
 *     runwith     Korri's runner choice is open (it opens it itself on a
 *                 launch, or on its own game action)
 *     games | options | savedata   the tab
 *
 * The status layer is drawn over whichever it is.
 */
export type ZoneShown = "paused" | "join" | "onboarding" | "runwith" | "games" | "options" | "savedata"

export const runnerOpen = (korri: SurfaceModel): boolean =>
  korri.runnerChoice !== undefined && korri.runnerChoice._tag !== "Closed"

export function zoneShown(state: ZoneSurfaceState, korri: SurfaceModel): ZoneShown {
  if (korri.presentation.kind === "gameplay-overlay") return "paused"
  if (korri.onboarding !== undefined) return state.options.keyboard === undefined ? "onboarding" : "join"
  if (runnerOpen(korri)) return "runwith"
  return state.nav.tab
}

/** Korri's status covers the page: Busy, Problem and Confirm make it inert. */
const covered = (korri: SurfaceModel): boolean =>
  korri.status._tag === "Busy" || korri.status._tag === "Problem" || korri.status._tag === "Confirm"

/** The start, in the look the device kept (zone-look-preference.ts reads it; the default without). */
export function zoneSurfaceStart(korri: SurfaceModel, look: ZoneLook = ZONE_DEFAULT_LOOK): ZoneSurfaceState {
  const start: ZoneSurfaceState = {
    look,
    nav: ZONE_START,
    options: optionsStart(korri),
    runWith: RUN_WITH_START,
    identity: IDENTITY_START,
    status: ZONE_STATUS_START,
    overlay: ZONE_OVERLAY_START,
    onboarding: ZONE_ONBOARDING_START,
  }
  return update(start, { _tag: "KorriPublished" }, korri).model
}

/** A step that took no effect: the same model and nothing asked. */
const idle = <Model>(step: ZoneStep<Model>, before: Model): boolean => step.model === before && step.requests.length === 0

/** Collects one model's step into the whole. */
class Steps {
  state: ZoneSurfaceState
  readonly requests: ZoneRequest[] = []
  constructor(state: ZoneSurfaceState) {
    this.state = state
  }
  put<K extends keyof ZoneSurfaceState>(key: K, step: ZoneStep<ZoneSurfaceState[K]>): ZoneStep<ZoneSurfaceState[K]> {
    if (step.model !== this.state[key]) this.state = { ...this.state, [key]: step.model }
    this.requests.push(...step.requests)
    return step
  }
  done(): ZoneStep<ZoneSurfaceState> {
    return { model: this.state, requests: this.requests }
  }
}

export function update(state: ZoneSurfaceState, message: ZoneSurfaceMessage, korri: SurfaceModel): ZoneStep<ZoneSurfaceState> {
  const steps = new Steps(state)
  route(steps, message, korri)
  return steps.done()
}

function route(steps: Steps, message: ZoneSurfaceMessage, korri: SurfaceModel): void {
  const state = steps.state
  switch (message._tag) {
    case "KorriPublished":
      return published(steps, korri)
    case "PressedBack":
      return back(steps, korri)
    case "PressedMenu":
      return menu(steps, korri)
    case "PressedOptions":
      if (zoneShown(state, korri) === "games" && !covered(korri)) steps.put("nav", updateNav(state.nav, message, korri))
      return
    case "PressedSystem":
      return system(steps, korri)
    case "Games":
      return games(steps, message.message, korri)
    case "Options":
      lookRow(steps, message.message)
      steps.put("options", updateOptions(state.options, message.message, withLook(steps.state, korri)))
      if (message.message._tag === "FocusedRow") steps.put("nav", updateNav(steps.state.nav, { _tag: "FocusedPage" }, korri))
      return
    case "RunWith":
      steps.put("runWith", updateRunWith(state.runWith, message.message, korri))
      return
    case "SaveData":
      steps.put("identity", updateIdentity(state.identity, message.message, korri.identityManagement))
      if (message.message._tag === "FocusedSaveCard" || message.message._tag === "FocusedSecret") {
        steps.put("nav", updateNav(steps.state.nav, { _tag: "FocusedPage" }, korri))
      }
      return
    case "Status":
      steps.put("status", updateStatus(state.status, message.message, korri.status))
      return
    case "Paused":
      if (korri.presentation.kind === "gameplay-overlay") {
        steps.put("overlay", updateOverlay(state.overlay, message.message, korri.presentation))
      }
      return
    case "Onboarding":
      return onboarding(steps, message.message, korri)
  }
}

/**
 * OPTIONS' LOOK row is Zone's own: A or a step on it goes to the look, and
 * Korri hears nothing. The options model still takes the press (it moves
 * its cursor to the row), and its plan, with the new look, redraws the
 * row's value at once. Under a keyboard or a dialog the row is not reachable.
 */
function lookRow(steps: Steps, message: ZoneOptionsMessage): void {
  if (optionsLayer(steps.state.options) !== "none") return
  if (message._tag === "PressedRow" && message.rowId === ZONE_SYSTEM_LOOK) {
    steps.put("look", updateLook(steps.state.look, { _tag: "SteppedLook", direction: 1 }))
  } else if (message._tag === "SteppedRow" && message.rowId === ZONE_SYSTEM_LOOK) {
    steps.put("look", updateLook(steps.state.look, { _tag: "SteppedLook", direction: message.direction }))
  }
}

function published(steps: Steps, korri: SurfaceModel): void {
  const message = { _tag: "KorriPublished" } as const
  const state = steps.state
  steps.put("nav", updateNav(state.nav, message, korri))
  steps.put("options", updateOptions(state.options, message, withLook(steps.state, korri)))
  steps.put("runWith", updateRunWith(state.runWith, message, korri))
  steps.put("identity", updateIdentity(state.identity, message, korri.identityManagement))
  steps.put("status", updateStatus(state.status, message, korri.status))
  // PAUSED opens on its first control each time Korri presents it.
  steps.put("overlay", korri.presentation.kind === "gameplay-overlay"
    ? updateOverlay(state.overlay, message, korri.presentation)
    : { model: ZONE_OVERLAY_START, requests: [] })
  // First boot starts at CONTROLLERS whenever Korri asks for it again.
  steps.put("onboarding", korri.onboarding !== undefined
    ? updateOnboarding(state.onboarding, message, korri)
    : { model: ZONE_ONBOARDING_START, requests: [] })
}

/** Back: Korri's problem, then the page's own rule, then the GAMES tab. */
function back(steps: Steps, korri: SurfaceModel): void {
  const state = steps.state
  const message = { _tag: "PressedBack" } as const
  if (korri.status._tag === "Problem") {
    steps.put("status", updateStatus(state.status, message, korri.status))
    return
  }
  // Korri's work is cancelled by the action it publishes, never by Back,
  // and its question is answered only by its own actions.
  if (korri.status._tag === "Busy" || korri.status._tag === "Confirm") return
  switch (zoneShown(state, korri)) {
    case "paused":
      if (korri.presentation.kind === "gameplay-overlay") {
        steps.put("overlay", updateOverlay(state.overlay, message, korri.presentation))
      }
      return
    case "join":
      steps.put("options", updateOptions(state.options, message, withLook(steps.state, korri)))
      return
    case "onboarding":
      steps.put("onboarding", updateOnboarding(state.onboarding, message, korri))
      return
    case "runwith":
      steps.put("runWith", updateRunWith(state.runWith, message, korri))
      return
    case "games":
      steps.put("nav", updateNav(state.nav, message, korri))
      return
    case "options":
      if (!idle(steps.put("options", updateOptions(state.options, message, withLook(steps.state, korri))), state.options)) return
      steps.put("nav", updateNav(state.nav, message, korri))
      return
    case "savedata":
      if (!idle(steps.put("identity", updateIdentity(state.identity, message, korri.identityManagement)), state.identity)) return
      steps.put("nav", updateNav(state.nav, message, korri))
      return
  }
}

/** An identity step or Korri's identity dialog holds the cursor on SAVE DATA. */
function identityHolds(identity: IdentityModel, korri: SurfaceModel): boolean {
  const status = korri.identityManagement?.status._tag
  return status === "Problem" || status === "Switched"
    || (identity.flow._tag !== "idle" && identity.flow._tag !== "showSecret")
}

function menu(steps: Steps, korri: SurfaceModel): void {
  const state = steps.state
  if (covered(korri)) return
  const message = { _tag: "PressedMenu" } as const
  switch (zoneShown(state, korri)) {
    case "games":
      steps.put("nav", updateNav(state.nav, message, korri))
      return
    case "options":
      switch (optionsLayer(state.options)) {
        case "keyboard":
          steps.put("options", updateOptions(state.options, message, withLook(steps.state, korri)))
          return
        case "dialog":
          return
        case "none":
          steps.put("nav", updateNav(state.nav, message, korri))
          return
      }
      return
    case "savedata":
      if (!identityHolds(state.identity, korri)) steps.put("nav", updateNav(state.nav, message, korri))
      return
    case "join":
      // The join flow's keyboard: menu commits it, as on OPTIONS.
      steps.put("options", updateOptions(state.options, message, withLook(steps.state, korri)))
      return
    case "paused":
    case "onboarding":
    case "runwith":
      return
  }
}

function system(steps: Steps, korri: SurfaceModel): void {
  const state = steps.state
  if (covered(korri)) return
  switch (zoneShown(state, korri)) {
    case "games":
    case "options":
    case "savedata":
      break
    case "paused":
    case "join":
    case "onboarding":
    case "runwith":
      return
  }
  if (state.nav.tab === "savedata" && identityHolds(state.identity, korri)) return
  const nav = steps.put("nav", updateNav(state.nav, { _tag: "PressedSystem" }, korri)).model
  if (nav.optionsGroup !== "system") return
  steps.put("options", { model: focusSystemGroup(steps.state.options), requests: [] })
  steps.put("nav", updateNav(nav, { _tag: "LandedOptions" }, korri))
}

/**
 * The GAMES page and the tabs report in navigation's words. Idle belongs to
 * the zone select alone: it shows over GAMES, so it never starts while
 * another page or Korri's runner choice holds the screen.
 */
function games(steps: Steps, message: ZoneNavMessage, korri: SurfaceModel): void {
  const state = steps.state
  if (message._tag === "IdleElapsed" && zoneShown(state, korri) !== "games") return
  steps.put("nav", updateNav(state.nav, message, korri))
}

/**
 * First boot. A secured network Korri has no key for is handed to OPTIONS'
 * join flow, which raises its keyboard for the password; the screen shows it
 * until DONE sends the join or B puts it down.
 */
function onboarding(steps: Steps, message: ZoneOnboardingMessage, korri: SurfaceModel): void {
  if (korri.onboarding === undefined) return
  const next = steps.put("onboarding", updateOnboarding(steps.state.onboarding, message, korri)).model
  if (message._tag === "OnboardingJoin" && next.join === message.networkId) {
    const rowId = zoneNetworkRowId(message.networkId)
    steps.put("options", updateOptions(steps.state.options, { _tag: "PressedRow", rowId }, withLook(steps.state, korri)))
  }
}
