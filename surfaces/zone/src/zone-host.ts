/**
 * The one door between Zone and Korri.
 *
 * Only this file calls a host command or listens to host input
 * (.oxlintrc.json, rule 2). Everything else returns a ZoneRequest from an
 * update function or dispatches a message, so every effect Zone has on Korri
 * is visible in one switch.
 */
import type { SurfaceAction, SurfaceHost } from "@contracts/surface/korri-surface"
import type { ZoneHostButton } from "./state/messages"
import type { ZoneRequest } from "./state/requests"

/** Perform one request. */
export function performZoneRequest(host: SurfaceHost, request: ZoneRequest): void {
  switch (request._tag) {
    case "LaunchGame":
      return request.locationId === undefined
        ? host.launchGame(request.gameId)
        : host.launchGame(request.gameId, request.locationId)
    case "RunAction":
      return host.runAction(request.actionId)
    case "ChangeSetting":
      return host.changeSetting(request.settingId, request.value)
    case "DismissSettingsProblem":
      return host.dismissSettingsProblem()
    case "ExportIdentityBackup":
      return host.exportIdentityBackup(request.password, request.retiredPublicKey)
    case "SwitchIdentityFromBackup":
      return host.switchIdentityFromBackup(
        request.encryptedSecret, request.password, request.disposition, request.trustLossConfirmed,
      )
    case "SwitchIdentityToNip46":
      return host.switchIdentityToNip46(request.bunkerUri, request.disposition, request.trustLossConfirmed)
    case "DeleteRetiredIdentity":
      return host.deleteRetiredIdentity(request.publicKey, request.backupConfirmed)
    case "DismissIdentityStatus":
      return host.dismissIdentityStatus()
    case "RunGameAction":
      return host.runGameAction(request.gameId, request.actionId)
    case "InvokeGameplayControl":
      return host.invokeGameplayControl(request.controlId, request.value)
    case "DismissGameplayOverlay":
      return host.dismissGameplayOverlay()
    case "Retry":
      return host.retry()
    case "DismissProblem":
      return host.dismiss()
    case "Reload":
      return host.reload()
    case "ScanWifi":
      return host.scanWifi()
    // A password goes straight to the host and nowhere else: never logged.
    case "ConnectWifi":
      return request.password === undefined
        ? host.connectWifi(request.networkId)
        : host.connectWifi(request.networkId, request.password)
    case "ConnectHiddenWifi":
      return request.password === undefined
        ? host.connectHiddenWifi(request.ssid)
        : host.connectHiddenWifi(request.ssid, request.password)
    case "FinishOnboarding":
      return host.finishOnboarding()
    case "OpenStore":
      return host.openStore()
    case "SearchStore":
      return host.searchStore(request.providerId, request.query)
    case "OpenStoreEntry":
      return host.openStoreEntry(request.entryId)
  }
}

/** Korri's actions for one game: a read, not a command. Empty when it has none. */
export function readZoneGameActions(host: SurfaceHost, gameId: string): readonly SurfaceAction[] {
  return host.gameActions(gameId)
}

/** Deliver the host's four buttons as messages. Returns the unsubscribe. */
export function subscribeZoneHostButtons(
  host: SurfaceHost,
  dispatch: (message: ZoneHostButton) => void,
): () => void {
  const actions = {
    PressedBack: "back", PressedOptions: "options", PressedMenu: "menu", PressedSystem: "system",
  } as const
  const offs = (Object.keys(actions) as (keyof typeof actions)[])
    .map(button => host.input.on(actions[button], () => dispatch({ _tag: button })))
  return () => {
    for (const off of offs) off()
  }
}
