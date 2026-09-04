import { describe, expect, it } from "bun:test"
import type { KorriNativeBridgeSurface } from "@contracts/bridge/korri-native-bridge"
import {
  LaunchContributorKind,
  LaunchDisposition,
  LaunchForegroundKind,
  MoonlightImplementation,
} from "@contracts/generated/korrid"
import {
  createInMemoryLauncherBridge,
  createKorriNativeLauncherBridge,
  discoverResolvedMoonlight,
  reserveResolvedMoonlightLaunch,
} from "./launcher-bridge"

const localContext = {
  gameId: "game",
  title: "Game",
  contributors: [
    { kind: LaunchContributorKind.Launcher, id: "@korri:retroarch/retroarch" },
  ],
  foreground: {
    kind: LaunchForegroundKind.Component,
    packageName: "pkg",
    className: "Activity",
  },
}

const moonlightContext = {
  gameId: "game",
  title: "Game",
  contributors: [
    {
      kind: LaunchContributorKind.Transport,
      id: "@korri:moonlight/moonlight",
    },
  ],
  executor: { id: "android-moonlight", available: false },
  foreground: { kind: LaunchForegroundKind.ArtemisGame },
}

describe("createInMemoryLauncherBridge", () => {
  it("launches local specs through the configured in-memory bridge", async () => {
    const bridge = createInMemoryLauncherBridge()
    const spec = {
      launchId: "launch-1",
      launcherId: "retroarch",
      disposition: LaunchDisposition.Fresh,
      context: localContext,
      component: { packageName: "pkg", className: "Activity" },
      extras: {},
      directories: [],
      files: [],
      integrity: "opaque-signature",
    }

    expect(await bridge.launchLocal(spec)).toEqual({ _tag: "Launched" })
  })

  it("resolves configured local game asset URLs in browser fixtures", async () => {
    const bridge = createInMemoryLauncherBridge({
      localGameAssetUrls: {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png":
          "data:image/png;base64,fixture",
      },
    })

    expect(
      await bridge.localGameAssetUrl(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
      ),
    ).toEqual({ _tag: "Resolved", url: "data:image/png;base64,fixture" })
    expect(await bridge.localGameAssetUrl("missing.png")).toEqual({
      _tag: "Absent",
    })
  })

  it("fails local launches when configured to", async () => {
    const bridge = createInMemoryLauncherBridge({ behavior: "local-launch-fail" })
    const result = await bridge.launchLocal({
      launchId: "launch-2",
      launcherId: "retroarch",
      disposition: LaunchDisposition.Fresh,
      context: localContext,
      component: { packageName: "pkg", className: "Activity" },
      extras: {},
      directories: [],
      files: [],
      integrity: "opaque-signature",
    })
    expect(result).toMatchObject({
      _tag: "LaunchFailed",
      reason: "NotInstalled",
    })
  })
})

describe("resolved Moonlight bridge path", () => {
  const available = {
    _tag: "Available" as const,
    payload: {
      transportId: "@korri:moonlight/moonlight",
      implementation: MoonlightImplementation.Artemis,
      sunshineApp: "Korri Stream",
    },
  }
  const launchSpec = {
    launchId: "0123456789abcdef0123456789abcdef",
    transportId: "@korri:moonlight/moonlight",
    context: moonlightContext,
    implementation: MoonlightImplementation.Artemis,
    sunshineApp: "Korri Stream",
    hostUuid: "h1",
    appId: 7,
    integrity: "opaque-integrity",
  }

  it("invokes native discovery and start only after successful typed resolution", async () => {
    const calls: string[] = []
    const bridge = createInMemoryLauncherBridge({
      streamHosts: [{ uuid: "h1", name: "Office" }],
      streamApps: { h1: [{ id: 7, name: "Korri Stream" }] },
    })
    const recordingBridge = {
      ...bridge,
      async queryStreamHosts() {
        calls.push("query-hosts")
        return bridge.queryStreamHosts()
      },
      async queryStreamApps(hostUuid: string) {
        calls.push(`query-apps:${hostUuid}`)
        return bridge.queryStreamApps(hostUuid)
      },
      async startStream(spec: typeof launchSpec) {
        calls.push(`start:${spec.hostUuid}:${spec.appId}:${spec.integrity}`)
        return bridge.startStream(spec)
      },
    }

    const discovered = await discoverResolvedMoonlight(available, recordingBridge)
    expect(discovered).toMatchObject({
      resolution: available,
      streams: [
        {
          host: { uuid: "h1", name: "Office" },
          apps: {
            _tag: "StreamApps",
            items: [{ id: 7, name: "Korri Stream" }],
          },
        },
      ],
    })
    expect(calls).toEqual(["query-hosts", "query-apps:h1"])

    expect(
      await reserveResolvedMoonlightLaunch(
        available,
        {
          async moonlightLaunchPrepare(hostUuid, appId) {
            calls.push(`prepare:${hostUuid}:${appId}`)
            return { _tag: "Ok" as const, payload: launchSpec }
          },
        },
        "h1",
        7,
      ),
    ).toEqual({ _tag: "Ok", payload: launchSpec })
    expect(calls).toEqual([
      "query-hosts",
      "query-apps:h1",
      "prepare:h1:7",
    ])
  })

  it("queries apps for a newly discovered host with no stored server pin", async () => {
    const calls: string[] = []
    const discovery = await discoverResolvedMoonlight(available, {
      async queryStreamHosts() {
        return {
          _tag: "StreamHosts" as const,
          items: [{ uuid: "new-host", name: "New host" }],
        }
      },
      async queryStreamApps(hostUuid: string) {
        calls.push(hostUuid)
        return { _tag: "StreamApps" as const, items: [] }
      },
    })

    expect(calls).toEqual(["new-host"])
    expect(discovery.streams).toEqual([
      {
        host: { uuid: "new-host", name: "New host" },
        apps: { _tag: "StreamApps", items: [] },
      },
    ])
  })

  it("preserves native discovery and signing failure tags unchanged", async () => {
    const hostFailure = { _tag: "QueryFailed" as const, message: "db locked" }
    expect(
      await discoverResolvedMoonlight(available, {
        async queryStreamHosts() {
          return hostFailure
        },
        async queryStreamApps() {
          throw new Error("must not query apps after host failure")
        },
      }),
    ).toEqual({ resolution: available, streams: [], hostsResult: hostFailure })

    const appFailure = { _tag: "QueryFailed" as const, message: "no cache" }
    const discovery = await discoverResolvedMoonlight(available, {
      async queryStreamHosts() {
        return {
          _tag: "StreamHosts",
          items: [{ uuid: "h1", name: "Office" }],
        }
      },
      async queryStreamApps() {
        return appFailure
      },
    })
    expect(discovery.streams[0]?.apps).toEqual(appFailure)

    const signingFailure = {
      _tag: "Err" as const,
      payload: {
        code: "LocalConfigUnauthorized",
        message: "current configuration is unauthorized",
      },
    }
    expect(
      await reserveResolvedMoonlightLaunch(
        available,
        {
          async moonlightLaunchPrepare() {
            return signingFailure
          },
        },
        "h1",
        7,
      ),
    ).toEqual(signingFailure)
  })

  it("does not invoke native Artemis when Moonlight is unavailable", async () => {
    const calls: string[] = []
    const bridge = {
      ...createInMemoryLauncherBridge(),
      async queryStreamHosts() {
        calls.push("query-hosts")
        return { _tag: "StreamHosts" as const, items: [] }
      },
      async queryStreamApps(hostUuid: string) {
        calls.push(`query-apps:${hostUuid}`)
        return { _tag: "StreamApps" as const, items: [] }
      },
      async startStream(spec: typeof launchSpec) {
        calls.push(`start:${spec.hostUuid}:${spec.appId}`)
        return { _tag: "StreamStarted" as const }
      },
    }
    const unavailable = {
      _tag: "Unavailable" as const,
      payload: {
        code: "MoonlightUnavailable",
        message: "Moonlight is disabled or Artemis is unavailable",
      },
    }

    expect(await discoverResolvedMoonlight(unavailable, bridge)).toEqual({
      resolution: unavailable,
      streams: [],
    })
    expect(
      await reserveResolvedMoonlightLaunch(
        unavailable,
        {
          async moonlightLaunchPrepare() {
            throw new Error("must not prepare while unavailable")
          },
        },
        "h1",
        7,
      ),
    ).toEqual({
      _tag: "Err",
      payload: unavailable.payload,
    })
    expect(calls).toEqual([])
  })
})

describe("createKorriNativeLauncherBridge", () => {
  const nativeLaunchSpec = {
    launchId: "0123456789abcdef0123456789abcdef",
    transportId: "@korri:moonlight/moonlight",
    context: moonlightContext,
    implementation: MoonlightImplementation.Artemis,
    sunshineApp: "Korri Stream",
    hostUuid: "h1",
    appId: 7,
    integrity: "opaque-integrity",
  }
  const surface = (
    overrides: Partial<KorriNativeBridgeSurface>,
  ): KorriNativeBridgeSurface => ({
    launchLocal: () => JSON.stringify({ _tag: "Launched" }),
    localGameAssetUrl: () => JSON.stringify({ _tag: "Absent" }),
    queryStreamHosts: () =>
      JSON.stringify({
        _tag: "StreamHosts",
        items: [{ uuid: "h1", name: "Office" }],
      }),
    queryStreamApps: () =>
      JSON.stringify({
        _tag: "StreamApps",
        items: [{ id: 7, name: "Desktop" }],
      }),
    startStream: () => JSON.stringify({ _tag: "StreamStarted" }),
    korridPort: () => 43117,
    korridCapability: () => "test-capability",
    storageAccess: () => JSON.stringify({ _tag: "Granted" }),
    openStorageAccessSettings: () => JSON.stringify({ _tag: "Opened" }),
    overlayPermission: () => JSON.stringify({ _tag: "Enabled" }),
    openOverlaySettings: () => JSON.stringify({ _tag: "Opened" }),
    backgroundNotice: () => JSON.stringify({ _tag: "Visible" }),
    requestBackgroundNotice: () => JSON.stringify({ _tag: "Granted" }),
    openNotificationSettings: () => JSON.stringify({ _tag: "Opened" }),
    ownerBindingSnapshot: () => JSON.stringify({
      identity: { _tag: "Unowned", devicePublicKey: "22".repeat(32) },
      personSigner: { _tag: "Unavailable", message: "Choose a signer" },
      requestedAction: "Bind this device to your person identity",
    }),
    startOwnerBinding: () => JSON.stringify({
      identity: { _tag: "Unowned", devicePublicKey: "22".repeat(32) },
      personSigner: { _tag: "Pending", message: "Waiting" },
      requestedAction: "Bind this device to your person identity",
    }),
    systemInfo: () =>
      JSON.stringify({
        _tag: "SystemInfo",
        payload: {
          device: "RG405M",
          manufacturer: "Anbernic",
          androidRelease: "14",
          sdk: 34,
          appVersion: "1.0",
        },
      }),
    openGameFolderPicker: () =>
      JSON.stringify({ _tag: "Opened", generation: "picker-1" }),
    gameFolderPickerSnapshot: () =>
      JSON.stringify({
        version: 1,
        generation: "picker-1",
        state: { _tag: "Selected", receipt: "receipt-1" },
      }),
    acknowledgeGameFolderPicker: () =>
      JSON.stringify({ _tag: "Acknowledged", generation: "picker-2" }),
    bridgeVersion: () => 19,
    ...overrides,
  })

  it("serializes a launcher-neutral local spec to the native surface", async () => {
    let received = ""
    const bridge = createKorriNativeLauncherBridge(
      surface({
        launchLocal: specJson => {
          received = specJson
          return JSON.stringify({ _tag: "Launched" })
        },
      }),
    )
    const spec = {
      launchId: "launch-3",
      launcherId: "retroarch",
      disposition: LaunchDisposition.Fresh,
      context: localContext,
      component: { packageName: "pkg", className: "Activity" },
      extras: { ROM: "/rom" },
      directories: [],
      files: [],
      integrity: "opaque-signature",
    }
    expect(await bridge.launchLocal(spec)).toEqual({ _tag: "Launched" })
    expect(JSON.parse(received)).toEqual(spec)
  })

  it("decodes local game asset URL results from the native surface", async () => {
    const bridge = createKorriNativeLauncherBridge(
      surface({
        localGameAssetUrl: () =>
          JSON.stringify({
            _tag: "Resolved",
            url: "https://appassets.androidplatform.net/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
          }),
      }),
    )

    expect(
      await bridge.localGameAssetUrl(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
      ),
    ).toEqual({
      _tag: "Resolved",
      url: "https://appassets.androidplatform.net/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
    })
  })

  it("serializes the signed Moonlight launch spec to the native surface", async () => {
    let received = ""
    const bridge = createKorriNativeLauncherBridge(
      surface({
        startStream: specJson => {
          received = specJson
          return JSON.stringify({ _tag: "StreamStarted" })
        },
      }),
    )
    expect(await bridge.queryStreamHosts()).toEqual({
      _tag: "StreamHosts",
      items: [{ uuid: "h1", name: "Office" }],
    })
    expect(await bridge.queryStreamApps("h1")).toEqual({
      _tag: "StreamApps",
      items: [{ id: 7, name: "Desktop" }],
    })
    expect(await bridge.startStream(nativeLaunchSpec)).toEqual({
      _tag: "StreamStarted",
    })
    expect(JSON.parse(received)).toEqual(nativeLaunchSpec)
  })

  it("round-trips valid shell-state bridge payloads", async () => {
    for (const expected of [
      { _tag: "Granted" },
      { _tag: "NotRequired" },
      { _tag: "Denied" },
      { _tag: "QueryFailed", message: "storage probe failed" },
    ] as const) {
      const bridge = createKorriNativeLauncherBridge(
        surface({ storageAccess: () => JSON.stringify(expected) }),
      )
      expect(await bridge.storageAccess()).toEqual(expected)
    }

    for (const expected of [
      { _tag: "Opened" },
      { _tag: "Unavailable", message: "no storage settings" },
    ] as const) {
      const bridge = createKorriNativeLauncherBridge(
        surface({ openStorageAccessSettings: () => JSON.stringify(expected) }),
      )
      expect(await bridge.openStorageAccessSettings()).toEqual(expected)
    }

    for (const expected of [
      { _tag: "Enabled" },
      { _tag: "Disabled" },
      { _tag: "RestrictedOrUnavailable" },
    ] as const) {
      const bridge = createKorriNativeLauncherBridge(
        surface({ overlayPermission: () => JSON.stringify(expected) }),
      )
      expect(await bridge.overlayPermission()).toEqual(expected)
    }

    for (const expected of [
      { _tag: "Opened" },
      { _tag: "Unavailable", message: "accessibility settings restricted" },
    ] as const) {
      const bridge = createKorriNativeLauncherBridge(
        surface({ openOverlaySettings: () => JSON.stringify(expected) }),
      )
      expect(await bridge.openOverlaySettings()).toEqual(expected)
    }

    for (const expected of [{ _tag: "Visible" }, { _tag: "Hidden" }] as const) {
      const bridge = createKorriNativeLauncherBridge(
        surface({ backgroundNotice: () => JSON.stringify(expected) }),
      )
      expect(await bridge.backgroundNotice()).toEqual(expected)
    }

    for (const expected of [
      { _tag: "Granted" },
      { _tag: "Denied" },
      { _tag: "Prompted" },
      { _tag: "Unprompted" },
    ] as const) {
      const bridge = createKorriNativeLauncherBridge(
        surface({ requestBackgroundNotice: () => JSON.stringify(expected) }),
      )
      expect(await bridge.requestBackgroundNotice()).toEqual(expected)
    }

    for (const expected of [
      { _tag: "Opened" },
      { _tag: "Unavailable", message: "no notification settings" },
    ] as const) {
      const bridge = createKorriNativeLauncherBridge(
        surface({ openNotificationSettings: () => JSON.stringify(expected) }),
      )
      expect(await bridge.openNotificationSettings()).toEqual(expected)
    }
  })

  it("decodes the receipt-based game folder picker contract", async () => {
    const bridge = createKorriNativeLauncherBridge(surface({}))

    expect(await bridge.openGameFolderPicker()).toEqual({
      _tag: "Opened",
      generation: "picker-1",
    })
    expect(await bridge.gameFolderPickerSnapshot()).toEqual({
      version: 1,
      generation: "picker-1",
      state: { _tag: "Selected", receipt: "receipt-1" },
    })
    expect(await bridge.acknowledgeGameFolderPicker("picker-1")).toEqual({
      _tag: "Acknowledged",
      generation: "picker-2",
    })
  })

  it("configures browser fixture picker outcomes with receipts, not paths", async () => {
    const selected = createInMemoryLauncherBridge({
      gameFolderPicker: { _tag: "Selected", receipt: "fixture-receipt" },
    })
    await selected.openGameFolderPicker()
    await new Promise(resolve => setTimeout(resolve, 1))
    expect(await selected.gameFolderPickerSnapshot()).toMatchObject({
      state: { _tag: "Selected", receipt: "fixture-receipt" },
    })

    const cancelled = createInMemoryLauncherBridge({
      gameFolderPicker: { _tag: "Cancelled" },
    })
    await cancelled.openGameFolderPicker()
    await new Promise(resolve => setTimeout(resolve, 1))
    expect((await cancelled.gameFolderPickerSnapshot()).state._tag).toBe(
      "Cancelled",
    )

    const failed = createInMemoryLauncherBridge({
      gameFolderPicker: {
        _tag: "Problem",
        code: "FolderSelectionUnresolvable",
        message: "cloud folder",
      },
    })
    await failed.openGameFolderPicker()
    await new Promise(resolve => setTimeout(resolve, 1))
    expect(await failed.gameFolderPickerSnapshot()).toMatchObject({
      state: { _tag: "Problem", code: "FolderSelectionUnresolvable" },
    })
  })

  it("keeps browser picker openings single-flight until the result is acknowledged", async () => {
    const bridge = createInMemoryLauncherBridge({
      gameFolderPicker: { _tag: "Selected", receipt: "fixture-receipt" },
    })

    const opened = await bridge.openGameFolderPicker()
    const duplicate = await bridge.openGameFolderPicker()
    await new Promise(resolve => setTimeout(resolve, 1))
    const selected = await bridge.gameFolderPickerSnapshot()
    const afterSelected = await bridge.openGameFolderPicker()

    expect(opened).toMatchObject({ _tag: "Opened" })
    expect(duplicate).toEqual({
      _tag: "Busy",
      generation: opened._tag === "Opened" ? opened.generation : "",
      state: "Choosing",
    })
    expect(selected.state).toEqual({ _tag: "Selected", receipt: "fixture-receipt" })
    expect(afterSelected).toEqual({
      _tag: "Busy",
      generation: selected.generation,
      state: "Selected",
    })
  })

  it("decodes Android system information", async () => {
    const bridge = createKorriNativeLauncherBridge(surface({}))

    expect(await bridge.systemInfo()).toEqual({
      _tag: "SystemInfo",
      payload: {
        device: "RG405M",
        manufacturer: "Anbernic",
        androidRelease: "14",
        sdk: 34,
        appVersion: "1.0",
      },
    })
  })

  it("converts stream bridge explosions into tagged failures", async () => {
    const bridge = createKorriNativeLauncherBridge(
      surface({
        startStream: () => {
          throw new Error("bridge exploded")
        },
      }),
    )
    expect(await bridge.startStream(nativeLaunchSpec)).toMatchObject({
      _tag: "StreamFailed",
      reason: "StartFailed",
    })
  })

  it("converts malformed local launch native payloads into tagged failures", async () => {
    const bridge = createKorriNativeLauncherBridge(
      surface({
        launchLocal: () => {
          throw new Error("bridge exploded")
        },
      }),
    )
    expect(
      await bridge.launchLocal({
        launchId: "launch-4",
        launcherId: "retroarch",
        disposition: LaunchDisposition.Fresh,
        context: localContext,
        component: { packageName: "pkg", className: "Activity" },
        extras: {},
        directories: [],
        files: [],
        integrity: "opaque-signature",
      }),
    ).toMatchObject({
      _tag: "LaunchFailed",
      reason: "StartFailed",
      message: "bridge exploded",
    })
  })

  it("rejects wrong tags in shell-state bridge payloads", async () => {
    const bridge = createKorriNativeLauncherBridge(
      surface({
        storageAccess: () => JSON.stringify({ _tag: "Opened" }),
        openStorageAccessSettings: () => JSON.stringify({ _tag: "Granted" }),
        overlayPermission: () => JSON.stringify({ _tag: "Granted" }),
        openOverlaySettings: () => JSON.stringify({ _tag: "Granted" }),
        backgroundNotice: () => JSON.stringify({ _tag: "Granted" }),
        requestBackgroundNotice: () => JSON.stringify({ _tag: "Visible" }),
        openNotificationSettings: () => JSON.stringify({ _tag: "Denied" }),
      }),
    )

    expect((await bridge.storageAccess())._tag).toBe("QueryFailed")
    expect(await bridge.openStorageAccessSettings()).toMatchObject({
      _tag: "Unavailable",
    })
    expect(await bridge.overlayPermission()).toEqual({
      _tag: "RestrictedOrUnavailable",
    })
    expect(await bridge.openOverlaySettings()).toMatchObject({
      _tag: "Unavailable",
    })
    expect(await bridge.backgroundNotice()).toEqual({ _tag: "Hidden" })
    expect(await bridge.requestBackgroundNotice()).toEqual({
      _tag: "Unprompted",
    })
    expect(await bridge.openNotificationSettings()).toMatchObject({
      _tag: "Unavailable",
    })
  })

  it("rejects missing fields in shell-state bridge payloads", async () => {
    const bridge = createKorriNativeLauncherBridge(
      surface({
        storageAccess: () => JSON.stringify({ _tag: "QueryFailed" }),
        openStorageAccessSettings: () =>
          JSON.stringify({ _tag: "Unavailable" }),
        openNotificationSettings: () =>
          JSON.stringify({ _tag: "Unavailable" }),
      }),
    )

    expect((await bridge.storageAccess())._tag).toBe("QueryFailed")
    expect(await bridge.openStorageAccessSettings()).toMatchObject({
      _tag: "Unavailable",
    })
    expect(await bridge.openNotificationSettings()).toMatchObject({
      _tag: "Unavailable",
    })
  })
})
