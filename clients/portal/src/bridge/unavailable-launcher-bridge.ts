import type { OwnerBindingSnapshot } from "@contracts/bridge/korri-native-bridge"
import type { LauncherBridge } from "./launcher-bridge"

const message = "Native device operations are unavailable in this shell."

/** Real RPC access does not imply native launch, storage or signer authority. */
export function createUnavailableLauncherBridge(): LauncherBridge {
  const ownerBindingSnapshot = async (): Promise<OwnerBindingSnapshot> => ({
    identity: { _tag: "Invalid", reason: message },
    personSigner: { _tag: "Unavailable", message },
    requestedAction: message,
  })
  return {
    async launchLocal() {
      return { _tag: "LaunchFailed", reason: "UnsupportedLauncher", message }
    },
    async localGameAssetUrl() { return { _tag: "Absent" } },
    async queryStreamHosts() { return { _tag: "QueryFailed", message } },
    async queryStreamApps() { return { _tag: "QueryFailed", message } },
    async startStream() {
      return { _tag: "StreamFailed", reason: "StartFailed", message }
    },
    async storageAccess() { return { _tag: "QueryFailed", message } },
    async openStorageAccessSettings() { return { _tag: "Unavailable", message } },
    async overlayPermission() { return { _tag: "RestrictedOrUnavailable" } },
    async openOverlaySettings() { return { _tag: "Unavailable", message } },
    // No Android background notification or permission prompt exists here.
    async backgroundNotice() { return { _tag: "Hidden" } },
    async requestBackgroundNotice() { return { _tag: "Unprompted" } },
    async openNotificationSettings() { return { _tag: "Unavailable", message } },
    ownerBindingSnapshot,
    startOwnerBinding: ownerBindingSnapshot,
    async systemInfo() { return { _tag: "Unavailable", message } },
    async openGameFolderPicker() { return { _tag: "Unavailable", message } },
    // KorriGameFolderPickerState starts at generation 0. No picker starts here.
    async gameFolderPickerSnapshot() {
      return { version: 1, generation: "0", state: { _tag: "Idle" } }
    },
    async acknowledgeGameFolderPicker() {
      return { _tag: "Stale", generation: "0" }
    },
  }
}
