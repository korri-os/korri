/**
 * The one door between Pico and Korri.
 *
 * Only this file calls a host command or listens to host input
 * (.oxlintrc.json, rule 2). Everything else returns a PicoRequest from an
 * update function or dispatches a message, so every effect Pico has on Korri
 * is visible in one switch.
 */
import type { SurfaceHost } from "@contracts/surface/korri-surface"
import * as QRCode from "qrcode"
import { rememberPicoFont } from "./pico-font-preference"
import type { PicoFontId } from "./pico-fonts"
import type { PicoHostButton } from "./state/messages"
import type { PicoReply, PicoRequest } from "./state/requests"

/** Effects Pico performs on its own device, not through Korri. */
export interface PicoLocalEffects {
  readonly chooseFont: (font: PicoFontId) => void
}

/** Perform one request. A request that answers returns its reply. */
export function performPicoRequest(
  host: SurfaceHost,
  local: PicoLocalEffects,
  request: PicoRequest,
): Promise<PicoReply> | void {
  switch (request._tag) {
    case "LaunchGame":
      return request.locationId === undefined
        ? host.launchGame(request.gameId)
        : host.launchGame(request.gameId, request.locationId)
    case "RunAction":
      return host.runAction(request.actionId)
    case "RunGameAction":
      return host.runGameAction(request.gameId, request.actionId)
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
    case "InvokeGameplayControl":
      return host.invokeGameplayControl(request.controlId, request.value)
    case "DismissGameplayOverlay":
      // The overlay's update sends any step a range still holds first.
      return host.dismissGameplayOverlay()
    case "Retry":
      return host.retry()
    case "DismissProblem":
      return host.dismiss()
    case "Reload":
      return host.reload()
    case "ChooseFont":
      return local.chooseFont(request.font)
    case "RememberFont":
      return rememberPicoFont(request.font)
    case "RenderQr":
      return QRCode.toDataURL(request.text, { width: 384, margin: 2 })
        .then(dataUrl => ({ _tag: "RenderedQr", text: request.text, dataUrl }) as const)
    case "Wait":
      return new Promise(resolve => {
        setTimeout(() => resolve({ _tag: "Waited", token: request.token }), request.ms)
      })
  }
}

/** Deliver the host's buttons as messages. Returns the unsubscribe. */
export function subscribePicoHostButtons(
  host: SurfaceHost,
  dispatch: (message: PicoHostButton) => void,
  buttons: readonly PicoHostButton["_tag"][] = ["PressedBack", "PressedSystem", "PressedOptions", "PressedMenu"],
): () => void {
  const actions = { PressedBack: "back", PressedSystem: "system", PressedOptions: "options", PressedMenu: "menu" } as const
  const offs = buttons.map(button => host.input.on(actions[button], () => dispatch({ _tag: button })))
  return () => {
    for (const off of offs) off()
  }
}
