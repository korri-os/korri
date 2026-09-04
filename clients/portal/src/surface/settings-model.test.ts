import { describe, expect, it } from "bun:test"
import { SecretSettingStatus, type SettingsSnapshot } from "@contracts/generated/korrid"
import { type DeviceFacts, settingsFrom } from "./settings-model"

const configuration: SettingsSnapshot = {
  revision: "r1",
  deviceName: "usu",
  steamGridDbCredential: SecretSettingStatus.NotConfigured,
  plugins: [
    { id: "@korri:mgba", title: "mGBA", enabled: true },
    { id: "@korri:retroarch", title: "RetroArch", enabled: false },
  ],
}

const group = (facts: DeviceFacts, title: string) =>
  settingsFrom(facts).find(candidate => candidate.title === title)

describe("settingsFrom", () => {
  it("shows one controller-first setup action with the exact binding facts", () => {
    const owner = group(
      {
        ownerBinding: {
          identity: { _tag: "Unowned", devicePublicKey: "22".repeat(32) },
          personSigner: {
            _tag: "Unavailable",
            message: "Install a compatible NIP-55 signer such as Amber",
          },
          deviceFingerprint: "22".repeat(32),
          requestedAction: "Bind this device to your person identity",
          bindingUri: "nostrsigner:%7Btemplate%7D?type=sign_event",
          signerRequirement: "Install a compatible NIP-55 signer such as Amber",
        },
      },
      "Owner",
    )
    expect(owner?.items.map(item => item.id)).toEqual([
      "device-fingerprint",
      "owner-requested-action",
      "owner-binding-uri",
      "owner-binding",
    ])
    expect(owner?.items.filter(item => item.interaction).map(item => item.id)).toEqual([
      "owner-binding",
    ])
  })

  it("makes the device name a bounded text setting", () => {
    expect(group({ settings: configuration }, "Device")?.items[0]).toMatchObject({
      id: "device-name",
      value: "usu",
      interaction: { kind: "text", maxLength: 64 },
    })
  })

  it("publishes SteamGridDB as a write-only sensitive setting", () => {
    const item = group(
      {
        settings: {
          ...configuration,
          steamGridDbCredential: SecretSettingStatus.Configured,
        },
      },
      "Metadata",
    )?.items[0]
    expect(item).toMatchObject({
      id: "steamgriddb-credential",
      label: "SteamGridDB API key",
      value: "Configured",
      interaction: {
        kind: "sensitiveText",
        placeholder: "Paste API key",
        clearLabel: "Clear saved key",
      },
    })
  })

  it("exposes sensitive clearability only for configured credentials", () => {
    const notConfigured = group({ settings: configuration }, "Metadata")?.items[0]
    expect(notConfigured?.value).toBe("Not configured")
    expect(notConfigured?.interaction).not.toHaveProperty("clearLabel")
  })

  it("makes plugin enablement an On/Off choice", () => {
    expect(group({ settings: configuration }, "Plugins")?.items).toEqual([
      {
        id: "@korri:mgba",
        label: "mGBA",
        value: "On",
        interaction: {
          kind: "choice",
          choices: [
            { value: "true", label: "On" },
            { value: "false", label: "Off" },
          ],
        },
      },
      {
        id: "@korri:retroarch",
        label: "RetroArch",
        value: "Off",
        interaction: {
          kind: "choice",
          choices: [
            { value: "true", label: "On" },
            { value: "false", label: "Off" },
          ],
        },
      },
    ])
  })

  it("states permissions and links each one to Android", () => {
    expect(
      group(
        {
          overlay: { _tag: "Disabled" },
          storage: { _tag: "Denied" },
          notice: { _tag: "Hidden" },
        },
        "Permissions",
      )?.items,
    ).toEqual([
      {
        id: "gameplay-overlay",
        label: "Gameplay overlay",
        value: "Disabled",
        description: "Managed by Android",
        interaction: { kind: "action", actionId: "overlay-access" },
      },
      {
        id: "file-access",
        label: "File access",
        value: "Not granted",
        description: "Managed by Android",
        interaction: { kind: "action", actionId: "storage-access" },
      },
      {
        id: "background-notice",
        label: "Background notice",
        value: "Hidden",
        description: "Managed by Android",
        interaction: { kind: "action", actionId: "background-notice" },
      },
    ])
  })

  it("states when Android restricts or cannot offer the overlay grant", () => {
    expect(
      group(
        { overlay: { _tag: "RestrictedOrUnavailable" } },
        "Permissions",
      )?.items[0],
    ).toEqual({
      id: "gameplay-overlay",
      label: "Gameplay overlay",
      value: "Restricted or unavailable",
      description: "Android does not currently offer this grant",
      interaction: { kind: "action", actionId: "overlay-access" },
    })
  })

  it("does not call a failed permission query denied", () => {
    expect(
      group(
        { storage: { _tag: "QueryFailed", message: "boom" } },
        "Permissions",
      )?.items[0]?.value,
    ).toBe("Unknown")
  })

  it("counts available streaming devices and names them", () => {
    const streaming = group(
      {
        hosts: [
          { uuid: "a", name: "zao" },
          { uuid: "b", name: "stranger" },
        ],
      },
      "Streaming",
    )
    expect(streaming?.items.map(item => [item.label, item.value])).toEqual([
      ["Streaming devices", "2 devices"],
      ["zao", "Known"],
      ["stranger", "Known"],
    ])
  })

  it("composes game folder actions without exposing paths as contract fields", () => {
    const games = group(
      {
        localGameCount: 1,
        discovery: {
          generation: "discovery-1",
          state: { _tag: "Idle", payload: {} },
          locations: [
            { id: "loc-a", label: "GBA" },
            { id: "loc-b", label: "More games" },
          ],
          diagnostics: [],
        },
      },
      "Games",
    )
    expect(games?.items[0]).toMatchObject({
      id: "local-games",
      value: "1 game",
      description: "Declared in library.yaml",
    })
    expect(games?.items.map(item => [item.label, item.value])).toEqual([
      ["On this device", "1 game"],
      ["Folder scan", "Ready"],
      ["Add game folder", undefined],
      ["Rescan game folders", "2 folders"],
      ["GBA", "Registered"],
      ["More games", "Registered"],
    ])
    expect(games?.items[4]?.interaction).toMatchObject({
      kind: "action",
      actionId: "game-folder-remove:loc-a",
      destructive: true,
      confirmation: { confirmLabel: "Remove folder" },
    })
  })

  it("shows calm discovery progress and bounded problems", () => {
    expect(
      group(
        {
          discovery: {
            generation: "discovery-1",
            state: { _tag: "Scanning", payload: {} },
            locations: [],
            diagnostics: [],
          },
        },
        "Games",
      )?.items.find(item => item.id === "game-discovery-status")?.value,
    ).toBe("Scanning…")

    expect(
      group(
        {
          discovery: {
            generation: "discovery-2",
            state: { _tag: "Problem", payload: {} },
            locations: [],
            diagnostics: [{ code: "Folder", message: "Folder is unavailable" }],
          },
        },
        "Games",
      )?.items.find(item => item.id === "game-discovery-status")?.description,
    ).toBe("Folder is unavailable")
  })

  it("publishes Android, app, and korrid identity as read-only facts", () => {
    const system = group(
      {
        version: "korrid-v0",
        systemInfo: {
          _tag: "SystemInfo",
          payload: {
            device: "RG405M",
            manufacturer: "Anbernic",
            androidRelease: "14",
            sdk: 34,
            appVersion: "1.2.3",
          },
        },
      },
      "System information",
    )
    expect(system?.items.map(item => [item.label, item.value])).toEqual([
      ["Device", "Anbernic RG405M"],
      ["Android", "14 · SDK 34"],
      ["Korri app", "1.2.3"],
      ["korrid", "korrid-v0"],
    ])
    expect(system?.items.every(item => item.interaction === undefined)).toBe(true)
  })
})
