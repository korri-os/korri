/**
 * The one function that changes Pico's catalog navigation.
 *
 * Pure: (navigation, message, Korri's model) -> (navigation, requests). Every
 * navigation rule lives here, and a test can drive each one with plain values.
 */
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { isPicoFont } from "../pico-font-preference"
import { picoScreenViewFromModel } from "../pico-screen-view"
import { picoSessionReturnFromModel, picoSessionReturnOnPlay } from "../pico-session-return"
import { PICO_FONT_SETTING } from "../pico-settings-view"
import { isHostButton, type PicoMessage } from "./messages"
import { type Detail, type Home, type PicoNavigation, type Settings, topLayer } from "./navigation"
import { ask, type PicoStep, stay } from "./requests"
import { attractShowing, canAttract, runnerOpen } from "./shown"

export function update(nav: PicoNavigation, message: PicoMessage, korri: SurfaceModel): PicoStep<PicoNavigation> {
  const result = step(nav, message, korri)
  return { model: settleIdle(settleSession(result.model, korri), korri), requests: result.requests }
}

function step(nav: PicoNavigation, message: PicoMessage, korri: SurfaceModel): PicoStep<PicoNavigation> {
  // The runner picker covers everything Pico draws. Back cancels it; the other
  // host buttons wait for it to close.
  if (runnerOpen(korri) && isHostButton(message)) {
    return message._tag === "PressedBack" ? ask(nav, { _tag: "RunAction", actionId: "runner:cancel" }) : stay(nav)
  }

  // A press while attract shows wakes the screen and does nothing else.
  // Picking a device up must not start a game, cycle a layout or open
  // Settings: the first press is how the player gets the screen back.
  if (isHostButton(message) || message._tag === "PressedScreen" || message._tag === "MovedFocus") {
    const waking = message._tag !== "MovedFocus" && attractShowing(nav, korri)
    const activity = nav.idle.activity + 1
    nav = { ...nav, idle: { _tag: message._tag === "MovedFocus" ? nav.idle._tag : "Awake", activity } }
    if (waking || !isHostButton(message)) return stay(nav)
  }

  // Input acts on what is shown. While Korri shows a launch, a running game
  // or a failure, the host buttons do not change the screens hidden below it.
  // Back acknowledges a failure.
  if (isHostButton(message) && korri.status._tag !== "Browsing") {
    return message._tag === "PressedBack" && korri.status._tag === "Problem"
      ? ask(nav, { _tag: "DismissProblem" })
      : stay(nav)
  }

  switch (message._tag) {
    case "PressedBack":
      return stay(back(nav))

    case "PressedSystem":
      // Settings toggles. Closing it drops its question, identity dialog included.
      return stay(nav.settings !== undefined
        ? { ...nav, settings: undefined }
        : { ...nav, settings: { question: { _tag: "None" } }, home: closeMenu(nav.home) })

    case "PressedOptions": {
      // Find opens from home and closes from Find. Over any other screen the
      // toggle would change only what is hidden below it.
      const top = topLayer(nav)
      if (top === "Find") return stay({ ...nav, find: false })
      if (top === "Home") return stay({ ...nav, find: true, home: closeMenu(nav.home) })
      return stay(nav)
    }

    case "PressedMenu":
      // The layout belongs to home. Cycling it under another screen changes nothing the player sees.
      return stay(topLayer(nav) === "Home" ? cycleMode(nav) : nav)

    case "KorriPublished":
      // settleSession and settleIdle, after every message, do the work.
      return stay(nav)

    case "IdleElapsed":
      // A timer that fired late cannot start attract over a screen that no longer allows it.
      return stay(nav.idle._tag === "Awake" && canAttract(nav, korri)
        ? { ...nav, idle: { _tag: "Attracting", activity: nav.idle.activity } }
        : nav)

    case "ChoseRunnerAction":
      return ask(nav, { _tag: "RunAction", actionId: message.actionId })

    // Home
    case "ToggledHomeMenu":
      return stay({ ...nav, home: { ...nav.home, menu: nav.home.menu === "Open" ? "Closed" : "Open" } })
    case "ChoseMenuFind":
      return stay({ ...nav, find: true, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } })
    case "ChoseMenuSettings":
      return stay({ ...nav, settings: { question: { _tag: "None" } }, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } })
    case "ChoseMenuView":
      return stay(cycleMode(nav))
    case "ReturnedFocus":
      return stay({ ...nav, home: { ...nav.home, focusOnReturn: "None" } })
    case "SelectedCart":
      return stay({ ...nav, home: { ...nav.home, selectedGameId: message.gameId } })
    case "OpenedGame": {
      const detail: Detail = { gameId: message.gameId, question: { _tag: "None" } }
      // From the shelf, Back should put the cursor on this cart again.
      return stay(topLayer(nav) === "Home"
        ? { ...nav, detail, home: { ...closeMenu(nav.home), focusOnReturn: nav.home.mode === "shelf" ? "Cart" : "None" } }
        : { ...nav, detail })
    }
    case "PressedRetry":
      return ask(nav, picoScreenViewFromModel(korri)._tag === "Problem" ? { _tag: "Retry" } : { _tag: "Reload" })
    case "PressedDismiss":
      return ask(nav, { _tag: "DismissProblem" })
    case "PressedStatusAction":
      // Only what Korri publishes now, enabled, is authority. A press that
      // lands after the work ended or the action went inert sends nothing.
      return korri.status._tag === "Busy" &&
        (korri.status.actions ?? []).some(action => action.id === message.actionId && action.enabled)
        ? ask(nav, { _tag: "RunAction", actionId: message.actionId })
        : stay(nav)

    // Find
    case "TypedCharacter":
      return stay({ ...nav, search: { ...nav.search, query: nav.search.query + message.character } })
    case "PressedBackspace":
      return stay({ ...nav, search: { ...nav.search, query: nav.search.query.slice(0, -1) } })
    case "ClearedQuery":
      return stay({ ...nav, search: { ...nav.search, query: "" } })
    case "ChoseSection":
      return stay({ ...nav, search: { ...nav.search, section: message.section } })
    case "ChoseOrder":
      return stay({ ...nav, search: { ...nav.search, order: message.order } })

    // A game's own screen
    case "PressedPlay": {
      const detail = nav.detail
      if (detail === undefined) return stay(nav)
      const view = picoScreenViewFromModel(korri)
      const game = view._tag === "Shelf" ? view.games.find(candidate => candidate.id === detail.gameId) : undefined
      if (game === undefined) return stay(nav)
      // Korri offers more than one place to play: ask, never pick for the player.
      if (game.locations !== undefined && game.locations.length > 0) {
        return stay({ ...nav, detail: { ...detail, question: { _tag: "ChoosingLocation", game } } })
      }
      return ask(
        { ...nav, session: picoSessionReturnOnPlay(nav.session, game.id, korri.catalog) },
        { _tag: "LaunchGame", gameId: game.id },
      )
    }
    case "ChoseLocation": {
      const detail = nav.detail
      if (detail?.question._tag !== "ChoosingLocation") return stay(nav)
      const gameId = detail.question.game.id
      return ask(
        {
          ...nav,
          detail: { ...detail, question: { _tag: "None" } },
          session: picoSessionReturnOnPlay(nav.session, gameId, korri.catalog),
        },
        { _tag: "LaunchGame", gameId, locationId: message.locationId },
      )
    }
    case "PressedGameAction": {
      const detail = nav.detail
      if (detail === undefined) return stay(nav)
      return message.action.destructive === true
        ? stay({ ...nav, detail: { ...detail, question: { _tag: "ConfirmingAction", action: message.action } } })
        : ask(nav, { _tag: "RunGameAction", gameId: detail.gameId, actionId: message.action.id })
    }
    case "ConfirmedGameAction": {
      const detail = nav.detail
      if (detail?.question._tag !== "ConfirmingAction") return stay(nav)
      return ask(
        { ...nav, detail: { ...detail, question: { _tag: "None" } } },
        { _tag: "RunGameAction", gameId: detail.gameId, actionId: detail.question.action.id },
      )
    }
    case "CancelledGameAction":
      return stay(nav.detail === undefined ? nav : { ...nav, detail: { ...nav.detail, question: { _tag: "None" } } })

    // Settings
    case "PressedSettingAction":
      return message.actionId.startsWith("identity:")
        ? stay(withSettings(nav, { question: { _tag: "Identity", actionId: message.actionId } }))
        : ask(nav, { _tag: "RunAction", actionId: message.actionId })
    case "AskedSettingConfirmation":
      return stay(withSettings(nav, {
        question: { _tag: "ConfirmingAction", actionId: message.actionId, confirmation: message.confirmation },
      }))
    case "ConfirmedSettingAction": {
      const question = nav.settings?.question
      if (question?._tag !== "ConfirmingAction") return stay(nav)
      return ask(withSettings(nav, { question: { _tag: "None" } }), { _tag: "RunAction", actionId: question.actionId })
    }
    case "CancelledSettingAction":
    case "ClosedEditor":
    case "ClosedIdentity":
      return stay(withSettings(nav, { question: { _tag: "None" } }))
    case "OpenedEditor":
      return stay(withSettings(nav, { question: { _tag: "EditingText", settingId: message.settingId } }))
    case "AskedClear": {
      const question = nav.settings?.question
      return question?._tag === "EditingText"
        ? stay(withSettings(nav, { question: { _tag: "ConfirmingClear", settingId: question.settingId } }))
        : stay(nav)
    }
    case "CancelledClear": {
      const question = nav.settings?.question
      return question?._tag === "ConfirmingClear"
        ? stay(withSettings(nav, { question: { _tag: "EditingText", settingId: question.settingId } }))
        : stay(nav)
    }
    case "ChangedSetting":
      // Pico's own row: Pico keeps it, and Korri is never asked.
      if (message.settingId === PICO_FONT_SETTING) {
        return isPicoFont(message.value) ? ask(nav, { _tag: "ChooseFont", font: message.value }) : stay(nav)
      }
      return ask(nav, { _tag: "ChangeSetting", settingId: message.settingId, value: message.value })
    case "DismissedSettingsProblem":
      return ask(nav, { _tag: "DismissSettingsProblem" })

    // The identity dialog
    case "SubmittedIdentityExport":
      return ask(nav, { _tag: "ExportIdentityBackup", password: message.password, retiredPublicKey: message.retiredPublicKey })
    case "SubmittedIdentityFromBackup":
      return ask(nav, {
        _tag: "SwitchIdentityFromBackup",
        encryptedSecret: message.encryptedSecret,
        password: message.password,
        disposition: message.disposition,
        trustLossConfirmed: message.trustLossConfirmed,
      })
    case "SubmittedIdentityToNip46":
      return ask(nav, {
        _tag: "SwitchIdentityToNip46",
        bunkerUri: message.bunkerUri,
        disposition: message.disposition,
        trustLossConfirmed: message.trustLossConfirmed,
      })
    case "SubmittedRetiredIdentityDeletion":
      return ask(nav, { _tag: "DeleteRetiredIdentity", publicKey: message.publicKey, backupConfirmed: message.backupConfirmed })
    case "DismissedIdentityStatus":
      return ask(nav, { _tag: "DismissIdentityStatus" })
  }
}

/**
 * Back while browsing withdraws the top layer's question, else closes the top
 * layer. Only the top layer: it never touches what the player cannot see.
 * With nothing left to close, leaving the surface is the host's decision.
 */
function back(nav: PicoNavigation): PicoNavigation {
  switch (topLayer(nav)) {
    case "Settings":
      return nav.settings === undefined ? nav : backInSettings(nav, nav.settings)
    case "Detail":
      if (nav.detail === undefined) return nav
      return nav.detail.question._tag !== "None"
        ? { ...nav, detail: { ...nav.detail, question: { _tag: "None" } } }
        : { ...nav, detail: undefined }
    case "Find":
      return { ...nav, find: false }
    case "Home":
      return nav.home.menu === "Open" ? { ...nav, home: closeMenu(nav.home) } : nav
  }
}

/** The clearing question, then the editor, then any other question, then Settings. */
function backInSettings(nav: PicoNavigation, settings: Settings): PicoNavigation {
  const question = settings.question
  switch (question._tag) {
    case "ConfirmingClear":
      return withSettings(nav, { question: { _tag: "EditingText", settingId: question.settingId } })
    case "ConfirmingAction":
    case "EditingText":
    case "Identity":
      return withSettings(nav, { question: { _tag: "None" } })
    case "None":
      return { ...nav, settings: undefined }
  }
}

/**
 * When a game played from its own screen ends, the player returns to the
 * shelf, with every screen and question closed (pico-session-return.ts says
 * what counts as an end).
 */
function settleSession(nav: PicoNavigation, korri: SurfaceModel): PicoNavigation {
  const session = picoSessionReturnFromModel(nav.session, nav.detail?.gameId, korri)
  // Keep the same value when nothing changed, so React has nothing to redraw.
  if (session === nav.session || (session._tag === "Idle" && nav.session._tag === "Idle")) return nav
  if (session._tag !== "ReturnToLibrary") return { ...nav, session }
  return {
    ...nav,
    home: { ...nav.home, mode: "shelf", menu: "Closed", focusOnReturn: "None" },
    find: false,
    detail: undefined,
    settings: undefined,
    session: { _tag: "Idle" },
  }
}

/** Attract leaves as soon as the screen no longer allows it. */
function settleIdle(nav: PicoNavigation, korri: SurfaceModel): PicoNavigation {
  return nav.idle._tag === "Attracting" && !canAttract(nav, korri)
    ? { ...nav, idle: { _tag: "Awake", activity: nav.idle.activity } }
    : nav
}

const closeMenu = (home: Home): Home => (home.menu === "Closed" ? home : { ...home, menu: "Closed" })
const withSettings = (nav: PicoNavigation, settings: Settings): PicoNavigation =>
  nav.settings === undefined ? nav : { ...nav, settings }
const cycleMode = (nav: PicoNavigation): PicoNavigation => ({
  ...nav,
  home: { ...nav.home, mode: nav.home.mode === "shelf" ? "grid" : nav.home.mode === "grid" ? "hero" : "shelf" },
})
