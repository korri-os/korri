import { describe, expect, it } from "bun:test"
import { createUnavailableLauncherBridge } from "./unavailable-launcher-bridge"

describe("unavailable native launcher", () => {
  it("reports unknown hardware facts without a sample device or stream hosts", async () => {
    const bridge = createUnavailableLauncherBridge()
    expect((await bridge.systemInfo())._tag).toBe("Unavailable")
    expect((await bridge.storageAccess())._tag).toBe("QueryFailed")
    expect((await bridge.queryStreamHosts())._tag).toBe("QueryFailed")
    expect((await bridge.queryStreamApps("peer"))._tag).toBe("QueryFailed")
    expect((await bridge.localGameAssetUrl("cover"))._tag).toBe("Absent")
    expect((await bridge.overlayPermission())._tag).toBe("RestrictedOrUnavailable")
    expect((await bridge.backgroundNotice())._tag).toBe("Hidden")
  })

  it("does not claim that settings, grants or a picker opened", async () => {
    const bridge = createUnavailableLauncherBridge()
    expect((await bridge.openStorageAccessSettings())._tag).toBe("Unavailable")
    expect((await bridge.openOverlaySettings())._tag).toBe("Unavailable")
    expect((await bridge.openNotificationSettings())._tag).toBe("Unavailable")
    expect((await bridge.requestBackgroundNotice())._tag).toBe("Unprompted")
    expect((await bridge.openGameFolderPicker())._tag).toBe("Unavailable")
    expect(await bridge.gameFolderPickerSnapshot()).toEqual({
      version: 1, generation: "0", state: { _tag: "Idle" },
    })
    expect(await bridge.acknowledgeGameFolderPicker("0")).toEqual({
      _tag: "Stale", generation: "0",
    })
  })

  it("does not publish an invented owner or begin a signer interaction", async () => {
    const bridge = createUnavailableLauncherBridge()
    const initial = await bridge.ownerBindingSnapshot()
    expect(initial.identity._tag).toBe("Invalid")
    expect(initial.personSigner._tag).toBe("Unavailable")
    expect(initial.deviceFingerprint).toBeUndefined()
    expect(await bridge.startOwnerBinding()).toEqual(initial)
  })
})
