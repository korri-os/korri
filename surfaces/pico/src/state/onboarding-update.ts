/**
 * The one function that changes first-boot setup.
 *
 * Pure: (setup, message, Korri's model) -> (setup, requests). Every setup
 * rule lives here, and a test drives each one with plain values.
 *
 * The rules the person decided: Wi-Fi is required, unless the device has no
 * Wi-Fi or is on a cable; going on offline is offered only where no network
 * worked, and asks first. Back goes back one step; on Hello it does nothing.
 */
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import {
  canRescanGames,
  DEVICE_NAME_SETTING,
  deviceName,
  deviceNameSetting,
  GAME_RESCAN_ACTION,
  gamesScanning,
  isOffline,
  padsView,
  picoWifiView,
  gamesCanContinue,
  starterPackChoices,
  starterPackView,
  wifiDone,
} from "../pico-onboarding-view"
import { picoDraftBackspace, picoDraftType, picoTextDraft, type PicoTextDraft } from "../pico-text-draft"
import {
  type IdentityStep,
  type OnboardingMessage,
  type OnboardingNav,
  type OnboardingStep,
  WIFI_STEP,
  type WifiStep,
} from "./onboarding"
import { ask, type PicoRequest, type PicoStep, stay } from "./requests"
import {
  editEntry,
  entryMaxLength,
  HIDDEN_NAME_ENTRY,
  lastAttempt,
  onNetwork,
  pickNetwork,
  retryAttempt,
  sameAttempt,
  scannedNetworks,
  submitEntry,
  toggleShown,
  type WifiNext,
} from "./wifi"

export function updateOnboarding(nav: OnboardingNav, message: OnboardingMessage, korri: SurfaceModel): PicoStep<OnboardingNav> {
  const result = step(nav, message, korri)
  const settled = settle(result.model, korri)
  return { model: settled.model, requests: [...result.requests, ...settled.requests] }
}

function step(nav: OnboardingNav, message: OnboardingMessage, korri: SurfaceModel): PicoStep<OnboardingNav> {
  switch (message._tag) {
    case "KorriPublished":
    case "Waited":
      return stay(nav)
    case "RenderedQr": {
      const backup = nav.backup
      return backup?.qr?.text === message.text
        ? stay({ ...nav, backup: { ...backup, qr: { text: message.text, dataUrl: message.dataUrl } } })
        : stay(nav)
    }
    case "PressedBack":
      return back(nav, korri)
    case "Started":
      return nav.step === "hello" ? enterWifi(nav, korri) : stay(nav)
    case "Continued":
      return continued(nav, korri)

    // Wi-Fi
    case "PickedNetwork":
      return nav.step === "wifi" ? join(nav, korri, pickNetwork(korri, message.networkId)) : stay(nav)
    case "ChoseHiddenNetwork":
      return nav.step === "wifi" ? stay(withWifi(nav, { question: { _tag: "Entry", entry: HIDDEN_NAME_ENTRY } })) : stay(nav)
    case "ScannedNetworks":
      return nav.step === "wifi" ? scan(nav, korri) : stay(nav)
    case "TriedAgain":
      return nav.step === "wifi" ? join(seeFailure(nav, korri), korri, retryAttempt(korri)) : stay(nav)
    case "PickedAnotherNetwork":
    case "ReturnedToWifi":
      return nav.step === "wifi"
        ? stay(withWifi(seeFailure(nav, korri), { showing: "List", question: { _tag: "None" } }))
        : stay(nav)
    case "StayedConnected":
      return nav.step === "wifi" ? stay(withWifi(seeFailure(nav, korri), { showing: "Auto" })) : stay(nav)
    case "ChoseWifiToo":
      return nav.step === "wifi" ? scan(withWifi(nav, { showing: "List" }), korri) : stay(nav)
    case "ChoseOffline": {
      const view = picoWifiView(nav.wifi, korri)
      // Offered only where no network worked, and only while there is no connection.
      return nav.step === "wifi" && isOffline(korri)
        && (view._tag === "NoNetworks" || (view._tag === "Failed" && view.fallback._tag === "Offline"))
        ? stay(withWifi(nav, { question: { _tag: "Warning" } }))
        : stay(nav)
    }
    case "ConfirmedOffline": {
      const view = picoWifiView(nav.wifi, korri)
      if (nav.step !== "wifi" || view._tag !== "OfflineWarning") return stay(nav)
      // The Wi-Fi step keeps what it showed, so Back from Name finds it.
      return stay(enterName({ ...withWifi(nav, { question: { _tag: "None" } }), continuedOffline: true }, korri))
    }

    // The keyboard
    case "TypedText":
      return stay(editText(nav, korri, (draft, maxLength) => picoDraftType(draft, message.character, maxLength)))
    case "PressedTextBackspace":
      return stay(editText(nav, korri, draft => picoDraftBackspace(draft)))
    case "ClearedText":
      return stay(editText(nav, korri, draft => ({ ...draft, text: "" })))
    case "ToggledCapitals":
      return stay(editText(nav, korri, draft => ({ ...draft, capitals: !draft.capitals })))
    case "ToggledSymbols":
      return stay(editText(nav, korri, draft => ({ ...draft, symbols: !draft.symbols })))
    case "ToggledShown":
      return stay(toggleSecret(nav))
    case "SubmittedText":
      return submitText(nav, korri)
    case "DismissedSettingsProblem":
      return ask(nav, { _tag: "DismissSettingsProblem" })

    // Identity
    case "ChoseIdentity":
      return chooseIdentity(nav, korri, message.how)
    case "EditedIdentityField":
      return editIdentityField(nav, message.field)
    case "ToggledTrustLoss":
      return toggleTrustLoss(nav)
    case "SubmittedIdentity":
      return submitIdentity(nav, korri)
    case "OpenedBackup":
      return nav.step === "you" && nav.identity._tag === "Ready"
        ? ask({ ...nav, backup: { password: "", qr: undefined } }, { _tag: "DismissIdentityStatus" })
        : stay(nav)
    case "EditedBackupPassword":
      return nav.backup === undefined ? stay(nav) : stay({ ...nav, backup: { ...nav.backup, password: message.password } })
    case "SubmittedBackup":
      return nav.backup === undefined || nav.backup.password.trim() === ""
        || korri.identityManagement?.status._tag === "Working"
        ? stay(nav)
        : ask(nav, { _tag: "ExportIdentityBackup", password: nav.backup.password })
    case "ClosedBackup":
      return closeBackup(nav)

    // Pads
    case "ChangedSetting":
      return nav.step === "pads" ? ask(nav, { _tag: "ChangeSetting", settingId: message.settingId, value: message.value }) : stay(nav)

    // Games
    case "ScannedGames":
      return nav.step === "games" && canRescanGames(korri) && !gamesScanning(korri)
        ? ask(nav, { _tag: "RunAction", actionId: GAME_RESCAN_ACTION })
        : stay(nav)
    case "ChoseStarterPackAction":
      // Only an action Korri offers now, and only one it has not dimmed.
      return nav.step === "games"
        && starterPackChoices(starterPackView(korri)).some(action => action.id === message.actionId && action.enabled)
        ? ask(nav, { _tag: "RunAction", actionId: message.actionId })
        : stay(nav)

    // Play
    case "Finished":
      return nav.step === "play" && !nav.finishing ? ask({ ...nav, finishing: true }, { _tag: "FinishOnboarding" }) : stay(nav)
  }
}

/* ---- Moving between steps ---- */

const at = (nav: OnboardingNav, step: OnboardingStep): OnboardingNav => ({ ...nav, step })

/**
 * Start: the Wi-Fi step. With Wi-Fi to set up it scans at once. A host that
 * cannot see its network has nothing to ask, so setup goes on to the name.
 */
function enterWifi(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  if (korri.network === undefined) return stay(enterName(nav, korri))
  const next = { ...at(nav, "wifi"), wifi: { ...nav.wifi, showing: "Auto" as const, question: { _tag: "None" as const } } }
  const wifi = korri.network.wifi
  return wifi._tag === "Available" && korri.network.connection._tag !== "Wired" ? scan(next, korri) : stay(next)
}

/** The name step. The first time, the field holds Korri's name for the device. */
function enterName(nav: OnboardingNav, korri: SurfaceModel): OnboardingNav {
  const draft = nav.name.draft ?? picoTextDraft(deviceName(korri) ?? "")
  return { ...at(nav, "name"), name: { draft, saving: "No" } }
}

function continued(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  switch (nav.step) {
    case "wifi": {
      const view = picoWifiView(nav.wifi, korri)
      if (!wifiDone(view)) return stay(nav)
      // A device with no Wi-Fi and no cable goes on offline.
      return stay(enterName({ ...nav, continuedOffline: nav.continuedOffline || isOffline(korri) }, korri))
    }
    case "you":
      return nav.identity._tag === "Ready" && nav.backup === undefined ? stay(at(nav, "pads")) : stay(nav)
    case "pads":
      return padsView(korri).ready ? stay(at(nav, "games")) : stay(nav)
    case "games":
      return gamesCanContinue(korri) ? stay(at(nav, "play")) : stay(nav)
    case "hello":
    case "name":
    case "play":
      return stay(nav)
  }
}

/** Back goes back one step, after closing whatever the step has open. */
function back(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  switch (nav.step) {
    case "hello":
      return stay(nav)
    case "wifi":
      return stay(backInWifi(nav, korri))
    case "name":
      return stay(korri.network === undefined ? at(nav, "hello") : at(nav, "wifi"))
    case "you":
      return backInIdentity(nav, korri)
    case "pads":
      return stay(at(nav, "you"))
    case "games":
      return stay(at(nav, "pads"))
    case "play":
      return nav.finishing ? stay(nav) : stay(at(nav, "games"))
  }
}

function backInWifi(nav: OnboardingNav, korri: SurfaceModel): OnboardingNav {
  const wifi = nav.wifi
  // A closed editor takes its password with it.
  if (wifi.question._tag !== "None") return withWifi(nav, { question: { _tag: "None" } })
  // The list while a connection works: back to that connection.
  const connection = korri.network?.connection
  if (wifi.showing === "List" && connection !== undefined && connection._tag !== "Offline") {
    return withWifi(nav, { showing: "Auto" })
  }
  return { ...at(nav, "hello"), wifi: { ...WIFI_STEP, seenFailure: wifi.seenFailure } }
}

function backInIdentity(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  if (nav.backup !== undefined) return closeBackup(nav)
  const identity = nav.identity
  switch (identity._tag) {
    case "Restore":
    case "Signer":
      if (identity.editing !== undefined) return stay(withIdentity(nav, { ...identity, editing: undefined }))
      // Korri is switching: there is no way to stop it, so Back waits.
      if (identity.sent && korri.identityManagement?.status._tag === "Working") return stay(nav)
      // The form closes, and any password typed into it goes too.
      return ask(withIdentity(nav, { _tag: "Choose" }), { _tag: "DismissIdentityStatus" })
    case "Choose":
    case "Ready":
      return stay(at(nav, "name"))
  }
}

/* ---- Wi-Fi ---- */

const withWifi = (nav: OnboardingNav, change: Partial<WifiStep>): OnboardingNav =>
  ({ ...nav, wifi: { ...nav.wifi, ...change } })

/** Ask for a scan, and show it as running until Korri answers. */
function scan(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  const wifi = korri.network?.wifi
  if (wifi?._tag !== "Available") return stay(nav)
  if (wifi.scanning) return stay(nav)
  return ask(
    withWifi(nav, { pending: { _tag: "Scan", saw: false, before: wifi.networks } }),
    { _tag: "ScanWifi" },
  )
}

/** The failure on screen is no longer news once the player acts on it. */
function seeFailure(nav: OnboardingNav, korri: SurfaceModel): OnboardingNav {
  const attempt = lastAttempt(korri)
  return attempt._tag === "Failed" ? withWifi(nav, { seenFailure: attempt }) : nav
}

/** A press answered: an editor opens, or the connect goes, the editor closes
 * with its password, and the step shows the connection under way. */
function join(nav: OnboardingNav, korri: SurfaceModel, next: WifiNext): PicoStep<OnboardingNav> {
  switch (next._tag) {
    case "Edit":
      return stay(withWifi(nav, { question: { _tag: "Entry", entry: next.entry } }))
    case "Send": {
      const ssid = joiningSsid(next.request, korri)
      return ask(
        withWifi(nav, {
          showing: "Auto",
          question: { _tag: "None" },
          pending: ssid === undefined ? undefined : { _tag: "Join", ssid, saw: false, before: lastAttempt(korri) },
        }),
        next.request,
      )
    }
    case "Nothing":
      return stay(nav)
  }
}

function joiningSsid(request: PicoRequest, korri: SurfaceModel): string | undefined {
  if (request._tag === "ConnectHiddenWifi") return request.ssid
  if (request._tag === "ConnectWifi") return scannedNetworks(korri).find(network => network.id === request.networkId)?.ssid
  return undefined
}

/* ---- The keyboard ---- */

/** Change what is typed, on whichever editor is open. */
function editText(
  nav: OnboardingNav,
  korri: SurfaceModel,
  change: (draft: PicoTextDraft, maxLength: number | undefined) => PicoTextDraft,
): OnboardingNav {
  switch (nav.step) {
    case "wifi": {
      const question = nav.wifi.question
      if (question._tag !== "Entry") return nav
      const entry = question.entry
      return withWifi(nav, { question: { _tag: "Entry", entry: editEntry(entry, draft => change(draft, entryMaxLength(entry))) } })
    }
    case "name": {
      if (nav.name.saving !== "No" || nav.name.draft === undefined) return nav
      return { ...nav, name: { ...nav.name, draft: change(nav.name.draft, deviceNameSetting(korri)?.maxLength) } }
    }
    case "you": {
      const identity = nav.identity
      if (identity._tag === "Restore" && identity.editing !== undefined) {
        return withIdentity(nav, { ...identity, editing: { ...identity.editing, draft: change(identity.editing.draft, undefined) } })
      }
      if (identity._tag === "Signer" && identity.editing !== undefined) {
        return withIdentity(nav, { ...identity, editing: change(identity.editing, undefined) })
      }
      return nav
    }
    case "hello":
    case "pads":
    case "games":
    case "play":
      return nav
  }
}

function toggleSecret(nav: OnboardingNav): OnboardingNav {
  if (nav.step === "wifi" && nav.wifi.question._tag === "Entry") {
    return withWifi(nav, { question: { _tag: "Entry", entry: toggleShown(nav.wifi.question.entry) } })
  }
  const identity = nav.identity
  if (nav.step === "you" && identity._tag === "Restore" && identity.editing?.field === "password") {
    return withIdentity(nav, { ...identity, editing: { ...identity.editing, shown: !identity.editing.shown } })
  }
  return nav
}

function submitText(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  switch (nav.step) {
    case "wifi": {
      const question = nav.wifi.question
      return question._tag === "Entry" ? join(nav, korri, submitEntry(question.entry)) : stay(nav)
    }
    case "name":
      return submitName(nav, korri)
    case "you": {
      const identity = nav.identity
      if (identity._tag === "Restore" && identity.editing !== undefined) {
        const { field, draft } = identity.editing
        return stay(withIdentity(nav, field === "backup"
          ? { ...identity, backup: draft.text.trim(), editing: undefined }
          : { ...identity, password: draft.text, editing: undefined }))
      }
      if (identity._tag === "Signer" && identity.editing !== undefined) {
        return stay(withIdentity(nav, { ...identity, uri: identity.editing.text.trim(), editing: undefined }))
      }
      return stay(nav)
    }
    case "hello":
    case "pads":
    case "games":
    case "play":
      return stay(nav)
  }
}

/**
 * NEXT on the name: save it through Korri's device-name setting and wait for
 * the save, as Settings' editor does. An unchanged name, or a host that
 * cannot change it, goes straight on.
 */
function submitName(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  const text = nav.name.draft?.text ?? ""
  if (text.trim() === "" || nav.name.saving !== "No") return stay(nav)
  if (deviceNameSetting(korri) === undefined || text === deviceName(korri)) return stay(at(nav, "you"))
  return ask({ ...nav, name: { ...nav.name, saving: "Sent" } }, { _tag: "ChangeSetting", settingId: DEVICE_NAME_SETTING, value: text })
}

/* ---- Identity ---- */

const withIdentity = (nav: OnboardingNav, identity: IdentityStep): OnboardingNav => ({ ...nav, identity })

/**
 * A new identity is the one this device already has: Korri made it, so
 * nothing is asked. Restoring and a signer need Korri's identity commands
 * and a connection; each opens its form with Korri's last status cleared.
 */
function chooseIdentity(nav: OnboardingNav, korri: SurfaceModel, how: "created" | "restored" | "signer"): PicoStep<OnboardingNav> {
  if (nav.step !== "you" || nav.identity._tag !== "Choose") return stay(nav)
  if (how === "created") return stay(withIdentity(nav, { _tag: "Ready", origin: "created" }))
  if (korri.identityManagement === undefined || isOffline(korri)) return stay(nav)
  return ask(
    withIdentity(nav, how === "restored"
      ? { _tag: "Restore", backup: "", password: "", editing: undefined, confirmed: false, sent: false }
      : { _tag: "Signer", uri: "", editing: undefined, confirmed: false, sent: false }),
    { _tag: "DismissIdentityStatus" },
  )
}

/** Open a field on the keyboard. Editing after a refusal clears it. */
function editIdentityField(nav: OnboardingNav, field: "backup" | "password" | "uri"): PicoStep<OnboardingNav> {
  const identity = nav.identity
  const cleared: PicoRequest[] = "sent" in identity && identity.sent ? [{ _tag: "DismissIdentityStatus" }] : []
  if (identity._tag === "Restore" && (field === "backup" || field === "password")) {
    return ask(withIdentity(nav, {
      ...identity,
      sent: false,
      editing: { field, draft: picoTextDraft(identity[field]), shown: false },
    }), ...cleared)
  }
  if (identity._tag === "Signer" && field === "uri") {
    return ask(withIdentity(nav, { ...identity, sent: false, editing: picoTextDraft(identity.uri) }), ...cleared)
  }
  return stay(nav)
}

/**
 * The player's answer to the trust-loss question. Setup also runs on a
 * device that has been in use, with pairings to lose, so only the player
 * may answer it; korrid refuses a switch without it.
 */
function toggleTrustLoss(nav: OnboardingNav): PicoStep<OnboardingNav> {
  const identity = nav.identity
  if (nav.step !== "you" || (identity._tag !== "Restore" && identity._tag !== "Signer")
    || identity.editing !== undefined || identity.sent) return stay(nav)
  return stay(withIdentity(nav, { ...identity, confirmed: !identity.confirmed }))
}

/**
 * RESTORE or CONNECT, as Settings sends them: only once the player has
 * confirmed the trust loss, and with that answer. Local records transfer.
 * The password goes with the request and leaves the form.
 */
function submitIdentity(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  const identity = nav.identity
  if (korri.identityManagement === undefined || korri.identityManagement.status._tag === "Working") return stay(nav)
  if (identity._tag === "Restore" && identity.editing === undefined && identity.confirmed
    && identity.backup.trim() !== "" && identity.password !== "") {
    return ask(withIdentity(nav, { ...identity, password: "", sent: true }), {
      _tag: "SwitchIdentityFromBackup",
      encryptedSecret: identity.backup.trim(),
      password: identity.password,
      disposition: "transfer",
      trustLossConfirmed: identity.confirmed,
    })
  }
  if (identity._tag === "Signer" && identity.editing === undefined && identity.confirmed && identity.uri !== "") {
    return ask(withIdentity(nav, { ...identity, sent: true }), {
      _tag: "SwitchIdentityToNip46",
      bunkerUri: identity.uri,
      disposition: "transfer",
      trustLossConfirmed: identity.confirmed,
    })
  }
  return stay(nav)
}

const closeBackup = (nav: OnboardingNav): PicoStep<OnboardingNav> =>
  nav.backup === undefined ? stay(nav) : ask({ ...nav, backup: undefined }, { _tag: "DismissIdentityStatus" })

/* ---- What Korri's model settles ---- */

/**
 * After every message: a scan or a connect Korri has answered stops being
 * pending; a saved name moves on; a switch Korri finished makes the identity
 * ready; a backup Korri made is drawn as a QR code; a host that stops
 * publishing its network leaves the Wi-Fi step.
 */
function settle(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  let next = settlePending(nav, korri)
  next = settleName(next, korri)
  next = settleIdentity(next, korri)
  if (next.step === "wifi" && korri.network === undefined) next = enterName(next, korri)
  return settleBackup(next, korri)
}

function settlePending(nav: OnboardingNav, korri: SurfaceModel): OnboardingNav {
  const pending = nav.wifi.pending
  if (pending === undefined) return nav
  const wifi = korri.network?.wifi
  if (wifi?._tag !== "Available") return withWifi(nav, { pending: undefined })
  switch (pending._tag) {
    case "Scan":
      if (wifi.scanning) return pending.saw ? nav : withWifi(nav, { pending: { ...pending, saw: true } })
      return pending.saw || !sameNetworks(wifi.networks, pending.before) ? withWifi(nav, { pending: undefined }) : nav
    case "Join": {
      if (onNetwork(korri, pending.ssid)) return withWifi(nav, { pending: undefined })
      if (wifi.attempt._tag === "Connecting") return pending.saw ? nav : withWifi(nav, { pending: { ...pending, saw: true } })
      return pending.saw || !sameAttempt(wifi.attempt, pending.before) ? withWifi(nav, { pending: undefined }) : nav
    }
  }
}

const sameNetworks = (a: readonly { readonly id: string }[], b: readonly { readonly id: string }[]): boolean =>
  a.length === b.length && a.every((network, index) => network.id === b[index]?.id)

function settleName(nav: OnboardingNav, korri: SurfaceModel): OnboardingNav {
  const saving = nav.name.saving
  if (saving === "No") return nav
  const status = korri.settingsStatus
  const mine = status._tag !== "Idle" && status.settingId === DEVICE_NAME_SETTING
  if (mine && status._tag === "Saving") return saving === "Seen" ? nav : { ...nav, name: { ...nav.name, saving: "Seen" } }
  // A refusal keeps the step and the typing; Korri's words show in the editor.
  if (mine && status._tag === "Problem") return { ...nav, name: { ...nav.name, saving: "No" } }
  const saved = saving === "Seen" || deviceName(korri) === nav.name.draft?.text
  return saved ? { ...at(nav, nav.step === "name" ? "you" : nav.step), name: { ...nav.name, saving: "No" } } : nav
}

function settleIdentity(nav: OnboardingNav, korri: SurfaceModel): OnboardingNav {
  const identity = nav.identity
  if ((identity._tag !== "Restore" && identity._tag !== "Signer") || !identity.sent) return nav
  if (korri.identityManagement?.status._tag !== "Switched") return nav
  return withIdentity(nav, { _tag: "Ready", origin: identity._tag === "Restore" ? "restored" : "signer" })
}

function settleBackup(nav: OnboardingNav, korri: SurfaceModel): PicoStep<OnboardingNav> {
  const backup = nav.backup
  if (backup === undefined) return stay(nav)
  const status = korri.identityManagement?.status
  const text = status?._tag === "BackupReady" ? status.encryptedSecret : undefined
  if (text === backup.qr?.text) return stay(nav)
  if (text === undefined) return stay({ ...nav, backup: { ...backup, qr: undefined } })
  return ask({ ...nav, backup: { ...backup, qr: { text, dataUrl: undefined } } }, { _tag: "RenderQr", text })
}
