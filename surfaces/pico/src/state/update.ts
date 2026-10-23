/**
 * The one function that changes Pico's catalog navigation.
 *
 * Pure: (navigation, message, Korri's model) -> (navigation, requests). Every
 * navigation rule lives here, and a test can drive each one with plain values.
 */
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { isPicoFont } from "../pico-font-preference"
import { PICO_INVITE_NOTICE_MS, picoInviteActionPublished } from "../pico-invite-view"
import { picoLaunchViewFromModel } from "../pico-launch-view"
import { picoScreenViewFromModel } from "../pico-screen-view"
import { picoSessionReturnFromModel, picoSessionReturnOnPlay } from "../pico-session-return"
import { PICO_STORE_QUERY_MAX } from "../pico-store-view"
import {
  PICO_FONT_SETTING,
  PICO_IDENTITY_BACKUP_ACTION,
  PICO_IDENTITY_SWITCH_LOCAL_ACTION,
  PICO_IDENTITY_SWITCH_NIP46_ACTION,
  PICO_WIFI_HIDDEN_ACTION,
  PICO_WIFI_SCAN_ACTION,
  picoIdentityRetiredKey,
  picoWifiNetworkId,
  picoSettingsViewFromModel,
  picoNumberFits,
  picoTextSettingFrom,
  type PicoSettingControl,
  type PicoSettingRowView,
  type PicoTextSettingRowView,
} from "../pico-settings-view"
import { picoDraftBackspace, picoDraftType, picoTextDraft, type PicoTextDraft } from "../pico-text-draft"
import { isHostButton, type PicoMessage } from "./messages"
import {
  type Detail,
  type Home,
  IDENTITY_FORM,
  type IdentityForm,
  type PicoNavigation,
  SETTINGS,
  type Settings,
  type SettingsQuestion,
  type StorePlace,
  type TextEditor,
  topLayer,
} from "./navigation"
import { ask, type PicoRequest, type PicoStep, stay } from "./requests"
import { attractShowing, canAttract, identityAction, invitesNoticeable, launchOpen, runnerOpen } from "./shown"
import {
  editEntry,
  entryMaxLength,
  HIDDEN_NAME_ENTRY,
  pickNetwork,
  submitEntry,
  toggleShown,
  type WifiEntry,
  type WifiNext,
} from "./wifi"

export function update(nav: PicoNavigation, message: PicoMessage, korri: SurfaceModel): PicoStep<PicoNavigation> {
  const result = step(settleLaunch(nav, korri), message, korri)
  const settled = settleIdle(settleEditor(settleLaunch(settleSession(result.model, korri), korri), korri), korri)
  const identity = settleIdentity(nav, settled, korri)
  const invites = settleInvites(identity.model, korri)
  return { model: invites.model, requests: [...result.requests, ...identity.requests, ...invites.requests] }
}

function step(nav: PicoNavigation, message: PicoMessage, korri: SurfaceModel): PicoStep<PicoNavigation> {
  // The runner picker covers everything Pico draws. Back cancels it; the other
  // host buttons wait for it to close.
  if (runnerOpen(korri) && isHostButton(message)) {
    return message._tag === "PressedBack" ? ask(nav, { _tag: "RunAction", actionId: "runner:cancel" }) : stay(nav)
  }

  // A game's launch settings cover everything too. Back withdraws its
  // question first, then asks Korri to close it; the other buttons wait.
  if (launchOpen(korri) && isHostButton(message)) {
    if (message._tag !== "PressedBack") return stay(nav)
    if (nav.launch !== undefined && nav.launch.question._tag !== "None") return stay(backInSettings(nav, nav.launch))
    const close = picoLaunchViewFromModel(korri)?.closeActionId
    return close === undefined ? stay(nav) : ask(nav, { _tag: "RunAction", actionId: close })
  }

  // A press while attract shows wakes the screen and does nothing else.
  // Picking a device up must not start a game, cycle a layout or open
  // Settings: the first press is how the player gets the screen back.
  // A press also ends the invite notices on screen: they were for a person
  // who was not using the device, and must never hide what a press does
  // (a letter typed in Find). The invites stay on home.
  if (isHostButton(message) || message._tag === "PressedScreen" || message._tag === "MovedFocus") {
    const pressed = message._tag !== "MovedFocus"
    const waking = pressed && attractShowing(nav, korri)
    const activity = nav.idle.activity + 1
    nav = {
      ...nav,
      idle: { _tag: pressed ? "Awake" : nav.idle._tag, activity },
      ...(pressed && nav.invites.showing.length > 0 ? { invites: { ...nav.invites, showing: [] } } : {}),
    }
    if (waking || !isHostButton(message)) return stay(nav)
  }

  // Input acts on what is shown. While Korri shows a launch, a running game
  // or a failure, the host buttons do not change the screens hidden below it.
  // Back acknowledges a failure, and a session that ended under this device.
  if (isHostButton(message) && korri.status._tag !== "Browsing") {
    return message._tag === "PressedBack" && (korri.status._tag === "Problem" || korri.status._tag === "Ended")
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
        : { ...nav, settings: SETTINGS, home: closeMenu(nav.home) })

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
    case "PressedLaunchAction":
      // Only what Korri publishes now, enabled, is authority.
      return picoLaunchViewFromModel(korri)?.actions.some(action => action.id === message.actionId && action.enabled) === true
        ? ask(nav, { _tag: "RunAction", actionId: message.actionId })
        : stay(nav)

    // Home
    case "ToggledHomeMenu":
      return stay({ ...nav, home: { ...nav.home, menu: nav.home.menu === "Open" ? "Closed" : "Open" } })
    case "ChoseMenuFind":
      return stay({ ...nav, find: true, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } })
    case "ChoseMenuSettings":
      return stay({ ...nav, settings: SETTINGS, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } })
    case "ChoseMenuView":
      return stay(cycleMode(nav))
    case "ChoseMenuStore":
      // Only where Korri can get games. Opening reads the providers again.
      return korri.store === undefined
        ? stay(nav)
        : ask({ ...nav, store: true, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } }, { _tag: "OpenStore" })
    case "AimedMenu":
      return stay(nav.home.aim === message.label ? nav : { ...nav, home: { ...nav.home, aim: message.label } })
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
      // A save split's sides are Confirm answers in the screen view.
      {
        const view = picoScreenViewFromModel(korri)
        return (view._tag === "Busy" || view._tag === "Confirm" || view._tag === "Paused") &&
          (view.actions ?? []).some(action => action.id === message.actionId && action.enabled)
          ? ask(nav, { _tag: "RunAction", actionId: message.actionId })
          : stay(nav)
      }

    // Invites. Only an action Korri publishes now, enabled, on an invite, and
    // only while this device is idle: a press that lands after the invite
    // went sends nothing.
    case "PressedInviteAction":
      return korri.status._tag === "Browsing" && picoInviteActionPublished(korri, message.actionId)
        ? ask(nav, { _tag: "RunAction", actionId: message.actionId })
        : stay(nav)
    case "LostInviteFocus":
      // The shelf takes the cursor back to the chosen cart. The other layouts
      // have no cart to return to; the next press finds the first control.
      return stay(topLayer(nav) === "Home" && nav.home.mode === "shelf"
        ? { ...nav, home: { ...nav.home, focusOnReturn: "Cart" } }
        : nav)

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

    // The store
    case "ChoseStoreProvider": {
      const providers = korri.store?.providers
      if (providers?._tag !== "Ready" || !providers.providers.some(provider => provider.id === message.providerId)) return stay(nav)
      // A provider not searched yet opens on the keyboard: typing is why you came.
      const search = korri.store?.search
      const searched = search !== undefined && search._tag !== "Idle" && search.providerId === message.providerId
      return stay(withStore(nav, {
        view: "Results",
        providerId: message.providerId,
        typing: searched ? undefined : picoTextDraft(""),
      }))
    }
    case "OpenedStoreSearch": {
      const search = korri.store?.search
      const query = search !== undefined && search._tag !== "Idle" && search.providerId === nav.storePlace.providerId ? search.query : ""
      return nav.storePlace.view === "Results" ? stay(withStore(nav, { ...nav.storePlace, typing: picoTextDraft(query) })) : stay(nav)
    }
    case "ClosedStoreSearch":
      return stay(withStore(nav, { ...nav.storePlace, typing: undefined }))
    case "SubmittedStoreSearch": {
      const { providerId, typing } = nav.storePlace
      if (providerId === undefined || typing === undefined) return stay(nav)
      return ask(withStore(nav, { ...nav.storePlace, typing: undefined }), { _tag: "SearchStore", providerId, query: typing.text.trim() })
    }
    case "OpenedStoreEntry": {
      const search = korri.store?.search
      if (search?._tag !== "Ready" || !search.entries.some(entry => entry.id === message.entryId)) return stay(nav)
      return ask(withStore(nav, { ...nav.storePlace, view: "Entry", typing: undefined }), { _tag: "OpenStoreEntry", entryId: message.entryId })
    }
    case "PressedStoreAction":
      // Only what Korri publishes now, enabled, is authority.
      return storeActionPublished(korri, message.actionId) ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav)
    case "PressedStorePlay": {
      const details = korri.store?.details
      if (details?._tag !== "Ready" || details.get._tag !== "InLibrary" || details.get.gameId !== message.gameId) return stay(nav)
      const view = picoScreenViewFromModel(korri)
      const game = view._tag === "Shelf" ? view.games.find(candidate => candidate.id === message.gameId) : undefined
      if (game === undefined) return stay(nav)
      // More than one place to play: the game's own screen asks.
      if (game.locations !== undefined && game.locations.length > 0) {
        return stay({ ...nav, detail: { gameId: game.id, question: { _tag: "ChoosingLocation", game } } })
      }
      return ask(
        { ...nav, session: picoSessionReturnOnPlay(nav.session, game.id, korri.catalog) },
        { _tag: "LaunchGame", gameId: game.id },
      )
    }

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
    case "ChoseSettingsGroup":
      if (nav.launch !== undefined) return stay({ ...nav, launch: { ...nav.launch, group: message.group } })
      return stay(nav.settings === undefined ? nav : { ...nav, settings: { ...nav.settings, group: message.group } })
    case "PressedSettingAction": {
      if (message.actionId.startsWith("identity:")) {
        return stay(withQuestion(nav, { _tag: "Identity", actionId: message.actionId, form: IDENTITY_FORM, qr: undefined }))
      }
      // Pico's own Wi-Fi rows go to the Wi-Fi commands, never to runAction.
      if (message.actionId === PICO_WIFI_SCAN_ACTION) {
        const wifi = korri.network?.wifi
        return wifi?._tag === "Available" && !wifi.scanning ? ask(nav, { _tag: "ScanWifi" }) : stay(nav)
      }
      if (message.actionId === PICO_WIFI_HIDDEN_ACTION) return stay(withQuestion(nav, { _tag: "Wifi", entry: HIDDEN_NAME_ENTRY }))
      const networkId = picoWifiNetworkId(message.actionId)
      if (networkId !== undefined) return joinFromSettings(nav, pickNetwork(korri, networkId))
      return ask(nav, { _tag: "RunAction", actionId: message.actionId })
    }
    case "AskedSettingConfirmation":
      return stay(withQuestion(nav, { _tag: "ConfirmingAction", actionId: message.actionId, confirmation: message.confirmation }))
    case "ConfirmedSettingAction": {
      const question = activeSettings(nav)?.question
      if (question?._tag !== "ConfirmingAction") return stay(nav)
      return ask(withQuestion(nav, { _tag: "None" }), { _tag: "RunAction", actionId: question.actionId })
    }
    case "ChoseLaunchScope":
      return stay(nav.launch === undefined ? nav : { ...nav, launch: { ...nav.launch, scope: message.scope } })
    case "OpenedPicker":
      return pickRow(korri, message.settingId) === undefined
        ? stay(nav)
        : stay(withQuestion(nav, { _tag: "PickingChoice", settingId: message.settingId }))
    case "PickedChoice": {
      // Only a value Korri offers on the row the list was opened from.
      const question = activeSettings(nav)?.question
      if (question?._tag !== "PickingChoice") return stay(nav)
      const row = pickRow(korri, question.settingId)
      if (row === undefined || !row.control.options.some(option => option.value === message.value)) {
        return stay(withQuestion(nav, { _tag: "None" }))
      }
      return ask(withQuestion(nav, { _tag: "None" }), { _tag: "ChangeSetting", settingId: question.settingId, value: message.value })
    }
    case "CancelledPicker":
    case "CancelledSettingAction":
    case "ClosedEditor":
    case "ClosedIdentity":
      return stay(withQuestion(nav, { _tag: "None" }))
    case "ChangedSetting":
      // Pico's own row: Pico keeps it, and Korri is never asked.
      if (message.settingId === PICO_FONT_SETTING) {
        return isPicoFont(message.value) ? ask(nav, { _tag: "ChooseFont", font: message.value }) : stay(nav)
      }
      return ask(nav, { _tag: "ChangeSetting", settingId: message.settingId, value: message.value })
    case "DismissedSettingsProblem":
      return ask(nav, { _tag: "DismissSettingsProblem" })

    // The text editor. Text starts from the value Korri published; a secret
    // starts empty. Typing is the editor's alone: a republished model keeps it.
    case "OpenedEditor": {
      const row = textRow(korri, message.settingId)
      if (row === undefined) return stay(nav)
      const draft = picoTextDraft(row.control.sensitive ? "" : (row.control.initial ?? row.value ?? ""))
      return stay(withQuestion(nav, { _tag: "EditingText", settingId: row.id, draft, sawSaving: false }))
    }
    case "AskedClear": {
      const editor = openEditor(nav)
      return editor?._tag === "EditingText" && !saving(korri, editor)
        ? stay(withQuestion(nav, { ...editor, _tag: "ConfirmingClear" }))
        : stay(nav)
    }
    case "CancelledClear": {
      const editor = openEditor(nav)
      return editor?._tag === "ConfirmingClear" ? stay(withQuestion(nav, { ...editor, _tag: "EditingText" })) : stay(nav)
    }
    case "ConfirmedClear": {
      const editor = openEditor(nav)
      if (editor?._tag !== "ConfirmingClear") return stay(nav)
      return ask(withQuestion(nav, { ...editor, _tag: "EditingText" }), { _tag: "ChangeSetting", settingId: editor.settingId, value: "" })
    }
    case "TypedText":
      return stay(editDraft(nav, korri, (draft, maxLength) => picoDraftType(draft, message.character, maxLength)))
    case "PressedTextBackspace":
      return stay(editDraft(nav, korri, draft => picoDraftBackspace(draft)))
    case "ClearedText":
      return stay(editDraft(nav, korri, draft => ({ ...draft, text: "" })))
    case "ToggledCapitals":
      return stay(editDraft(nav, korri, draft => ({ ...draft, capitals: !draft.capitals })))
    case "ToggledSymbols":
      return stay(editDraft(nav, korri, draft => ({ ...draft, symbols: !draft.symbols })))
    case "ToggledShown": {
      const entry = wifiEntry(nav)
      return entry === undefined ? stay(nav) : stay(withQuestion(nav, { _tag: "Wifi", entry: toggleShown(entry) }))
    }
    case "SubmittedWifi": {
      const entry = wifiEntry(nav)
      return entry === undefined ? stay(nav) : joinFromSettings(nav, submitEntry(entry))
    }
    case "SavedText": {
      // Save sends exactly what was typed, never an empty value unless Korri
      // takes one (allowEmpty): emptying a secret is the clear action's job.
      const editor = openEditor(nav)
      if (editor?._tag !== "EditingText" || saving(korri, editor)) return stay(nav)
      if (editor.draft.text.trim() === "" && textRow(korri, editor.settingId)?.control.allowEmpty !== true) return stay(nav)
      // A number goes to Korri only when it fits the range Korri gave.
      const numeric = textRow(korri, editor.settingId)?.control.numeric
      if (numeric !== undefined && !picoNumberFits(editor.draft.text, numeric)) return stay(nav)
      return ask(nav, { _tag: "ChangeSetting", settingId: editor.settingId, value: editor.draft.text })
    }

    // The identity dialog
    case "EditedIdentity":
      return stay(editIdentity(nav, form => ({ ...form, [message.field]: message.value })))
    case "ChoseIdentityDisposition":
      return stay(editIdentity(nav, form => ({ ...form, disposition: message.disposition })))
    case "CheckedIdentityConfirmation":
      return stay(editIdentity(nav, form => ({ ...form, confirmed: message.confirmed })))
    case "SubmittedIdentity": {
      const question = nav.settings?.question
      if (question?._tag !== "Identity" || korri.identityManagement?.status._tag === "Working") return stay(nav)
      const request = identityRequest(question.actionId, question.form)
      return request === undefined ? stay(nav) : ask(nav, request)
    }
    case "RenderedQr": {
      const question = nav.settings?.question
      return question?._tag === "Identity" && question.qr?.text === message.text
        ? stay(withQuestion(nav, { ...question, qr: { text: message.text, dataUrl: message.dataUrl } }))
        : stay(nav)
    }
    case "Waited":
      // A notice's time is up. The invite stays on home.
      return stay(nav.invites.showing.some(notice => notice.token === message.token)
        ? { ...nav, invites: { ...nav.invites, showing: nav.invites.showing.filter(notice => notice.token !== message.token) } }
        : nav)
  }
}

/**
 * Invites Korri publishes that have had no notice get one now, if home or
 * Find shows, each with its own wait. A notice goes when its time is up or
 * its invite goes.
 */
function settleInvites(nav: PicoNavigation, korri: SurfaceModel): PicoStep<PicoNavigation> {
  const standing = korri.invites ?? []
  const showing = nav.invites.showing.filter(notice => standing.some(invite => invite.id === notice.inviteId))
  const fresh = invitesNoticeable(nav, korri)
    ? standing.filter(invite => !nav.invites.noticed.includes(invite.id))
    : []
  if (fresh.length === 0 && showing.length === nav.invites.showing.length) return stay(nav)
  const added = fresh.map((invite, index) => ({ inviteId: invite.id, token: nav.invites.nextToken + index }))
  return {
    model: {
      ...nav,
      invites: {
        noticed: [...nav.invites.noticed, ...fresh.map(invite => invite.id)],
        // Newest first: the last one Korri lists is the newest here.
        showing: [...[...added].reverse(), ...showing],
        nextToken: nav.invites.nextToken + added.length,
      },
    },
    requests: added.map(notice => ({ _tag: "Wait" as const, ms: PICO_INVITE_NOTICE_MS, token: notice.token })),
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
    case "Store":
      return backInStore(nav, nav.storePlace)
    case "Find":
      return { ...nav, find: false }
    case "Home":
      return nav.home.menu === "Open" ? { ...nav, home: closeMenu(nav.home) } : nav
  }
}

/** The keyboard, then the entry, then the results, then the store itself. */
function backInStore(nav: PicoNavigation, place: StorePlace): PicoNavigation {
  if (place.typing !== undefined) return withStore(nav, { ...place, typing: undefined })
  switch (place.view) {
    case "Entry":
      return withStore(nav, { ...place, view: "Results" })
    case "Results":
      return withStore(nav, { ...place, view: "Providers" })
    case "Providers":
      return { ...nav, store: false }
  }
}

/** The clearing question, then the editor, then any other question, then Settings. */
function backInSettings(nav: PicoNavigation, settings: Settings): PicoNavigation {
  const question = settings.question
  switch (question._tag) {
    case "ConfirmingClear":
      return withQuestion(nav, { ...question, _tag: "EditingText" })
    case "ConfirmingAction":
    case "EditingText":
    case "PickingChoice":
    case "Identity":
    case "Wifi":
      return withQuestion(nav, { _tag: "None" })
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
    store: false,
    detail: undefined,
    settings: undefined,
    session: { _tag: "Idle" },
  }
}

/**
 * Korri reports a save. The editor closes when the row it saw saving is idle
 * again; a refusal keeps it open with the typing kept, so a second try does
 * not start over.
 */
function settleEditor(nav: PicoNavigation, korri: SurfaceModel): PicoNavigation {
  const editor = openEditor(nav)
  if (editor === undefined) return nav
  const state = textRow(korri, editor.settingId)?.state
  if (state === "saving") return editor.sawSaving ? nav : withQuestion(nav, { ...editor, sawSaving: true })
  if (!editor.sawSaving) return nav
  return withQuestion(nav, state === "idle" ? { _tag: "None" } : { ...editor, sawSaving: false })
}

/**
 * The identity dialog. Opening, changing or closing it clears the status Korri
 * shows for the last identity operation, so an old result never greets a new
 * one. A backup Korri made is drawn as a QR code.
 */
function settleIdentity(before: PicoNavigation, nav: PicoNavigation, korri: SurfaceModel): PicoStep<PicoNavigation> {
  const requests: PicoRequest[] = []
  if (identityAction(before) !== identityAction(nav)) requests.push({ _tag: "DismissIdentityStatus" })
  const question = nav.settings?.question
  if (question?._tag !== "Identity") return { model: nav, requests }
  const status = korri.identityManagement?.status
  const text = status?._tag === "BackupReady" ? status.encryptedSecret : undefined
  if (text === question.qr?.text) return { model: nav, requests }
  if (text === undefined) return { model: withQuestion(nav, { ...question, qr: undefined }), requests }
  requests.push({ _tag: "RenderQr", text })
  return { model: withQuestion(nav, { ...question, qr: { text, dataUrl: undefined } }), requests }
}

/** What a submitted identity form asks Korri to do, when the form is complete. */
function identityRequest(actionId: string, form: IdentityForm): PicoRequest | undefined {
  const exportKey = picoIdentityRetiredKey(actionId, "export")
  const deleteKey = picoIdentityRetiredKey(actionId, "delete")
  if (actionId === PICO_IDENTITY_BACKUP_ACTION || exportKey !== undefined) {
    return form.password.trim() === "" ? undefined
      : { _tag: "ExportIdentityBackup", password: form.password, retiredPublicKey: exportKey }
  }
  if (actionId === PICO_IDENTITY_SWITCH_LOCAL_ACTION) {
    return !form.confirmed || form.secret.trim() === "" || form.password.trim() === "" ? undefined : {
      _tag: "SwitchIdentityFromBackup",
      encryptedSecret: form.secret,
      password: form.password,
      disposition: form.disposition,
      trustLossConfirmed: form.confirmed,
    }
  }
  if (actionId === PICO_IDENTITY_SWITCH_NIP46_ACTION) {
    return !form.confirmed || form.bunkerUri.trim() === "" ? undefined : {
      _tag: "SwitchIdentityToNip46",
      bunkerUri: form.bunkerUri,
      disposition: form.disposition,
      trustLossConfirmed: form.confirmed,
    }
  }
  if (deleteKey !== undefined) {
    return form.confirmed ? { _tag: "DeleteRetiredIdentity", publicKey: deleteKey, backupConfirmed: form.confirmed } : undefined
  }
  return undefined
}

type PickRow = PicoSettingRowView & { readonly control: Extract<PicoSettingControl, { readonly kind: "pick" }> }

/** The pick row `settingId`, in launch settings while Korri shows them, else Settings. */
const pickRow = (korri: SurfaceModel, settingId: string): PickRow | undefined => {
  const groups = [...(picoLaunchViewFromModel(korri)?.settings.groups ?? []), ...picoSettingsViewFromModel(korri).groups]
  const row = groups.flatMap(group => group.rows).find(candidate => candidate.id === settingId)
  return row?.control.kind === "pick" ? row as PickRow : undefined
}

const textRow = (korri: SurfaceModel, settingId: string): PicoTextSettingRowView | undefined => {
  const launch = picoLaunchViewFromModel(korri)?.settings
  return (launch === undefined ? undefined : picoTextSettingFrom(launch, settingId)?.row)
    ?? picoTextSettingFrom(picoSettingsViewFromModel(korri), settingId)?.row
}

/**
 * Korri opened or closed a game's launch settings. Opening starts on the
 * first spine with no question; closing forgets both.
 */
function settleLaunch(nav: PicoNavigation, korri: SurfaceModel): PicoNavigation {
  const open = launchOpen(korri)
  if (open && nav.launch === undefined) return { ...nav, launch: SETTINGS }
  if (!open && nav.launch !== undefined) return { ...nav, launch: undefined }
  return nav
}

/** The settings layer questions go to: launch settings while Korri shows
 * them, else Settings. */
const activeSettings = (nav: PicoNavigation): Settings | undefined => nav.launch ?? nav.settings

type OpenEditor = Extract<SettingsQuestion, { readonly _tag: "EditingText" | "ConfirmingClear" }>

const openEditor = (nav: PicoNavigation): OpenEditor | undefined => {
  const question = activeSettings(nav)?.question
  return question?._tag === "EditingText" || question?._tag === "ConfirmingClear" ? question : undefined
}

const saving = (korri: SurfaceModel, editor: TextEditor): boolean =>
  textRow(korri, editor.settingId)?.state === "saving"

/**
 * Change what is typed: a text setting's value, or what joins a Wi-Fi
 * network. A setting's keys do nothing while Korri saves, or behind the clear
 * question. `maxLength` is the limit the keys enforce.
 */
function editDraft(
  nav: PicoNavigation,
  korri: SurfaceModel,
  change: (draft: PicoTextDraft, maxLength: number | undefined) => PicoTextDraft,
): PicoNavigation {
  const entry = wifiEntry(nav)
  if (entry !== undefined) {
    return withQuestion(nav, { _tag: "Wifi", entry: editEntry(entry, draft => change(draft, entryMaxLength(entry))) })
  }
  // The store's keyboard, when it is the one on screen.
  const typing = nav.storePlace.typing
  if (topLayer(nav) === "Store" && typing !== undefined) {
    return withStore(nav, { ...nav.storePlace, typing: change(typing, PICO_STORE_QUERY_MAX) })
  }
  const editor = openEditor(nav)
  if (editor?._tag !== "EditingText") return nav
  const row = textRow(korri, editor.settingId)
  if (row === undefined || row.state === "saving") return nav
  return withQuestion(nav, { ...editor, draft: change(editor.draft, row.control.maxLength) })
}

const wifiEntry = (nav: PicoNavigation): WifiEntry | undefined => {
  const question = nav.settings?.question
  return question?._tag === "Wifi" ? question.entry : undefined
}

/** A Wi-Fi press answered: an editor opens, or the connect goes and the
 * editor closes, taking any password with it. */
function joinFromSettings(nav: PicoNavigation, next: WifiNext): PicoStep<PicoNavigation> {
  switch (next._tag) {
    case "Edit":
      return stay(withQuestion(nav, { _tag: "Wifi", entry: next.entry }))
    case "Send":
      return ask(withQuestion(nav, { _tag: "None" }), next.request)
    case "Nothing":
      return stay(nav)
  }
}

function editIdentity(nav: PicoNavigation, change: (form: IdentityForm) => IdentityForm): PicoNavigation {
  const question = nav.settings?.question
  return question?._tag === "Identity" ? withQuestion(nav, { ...question, form: change(question.form) }) : nav
}

/** Attract leaves as soon as the screen no longer allows it. */
function settleIdle(nav: PicoNavigation, korri: SurfaceModel): PicoNavigation {
  return nav.idle._tag === "Attracting" && !canAttract(nav, korri)
    ? { ...nav, idle: { _tag: "Awake", activity: nav.idle.activity } }
    : nav
}

const withStore = (nav: PicoNavigation, place: StorePlace): PicoNavigation => ({ ...nav, storePlace: place })

/** Whether Korri publishes this store action now, enabled. */
function storeActionPublished(korri: SurfaceModel, actionId: string): boolean {
  const store = korri.store
  if (store === undefined) return false
  const lists = [
    store.providers._tag === "Problem" ? store.providers.actions : [],
    store.search._tag === "Problem" ? store.search.actions : [],
    store.details._tag === "Problem" ? store.details.actions : [],
    store.details._tag === "Ready" && "actions" in store.details.get ? store.details.get.actions : [],
  ]
  return lists.some(actions => actions.some(action => action.id === actionId && action.enabled))
}

const closeMenu = (home: Home): Home => (home.menu === "Closed" ? home : { ...home, menu: "Closed" })
const withQuestion = (nav: PicoNavigation, question: SettingsQuestion): PicoNavigation =>
  nav.launch !== undefined
    ? { ...nav, launch: { ...nav.launch, question } }
    : nav.settings === undefined ? nav : { ...nav, settings: { ...nav.settings, question } }
const cycleMode = (nav: PicoNavigation): PicoNavigation => ({
  ...nav,
  home: { ...nav.home, mode: nav.home.mode === "shelf" ? "grid" : nav.home.mode === "grid" ? "hero" : "shelf" },
})
