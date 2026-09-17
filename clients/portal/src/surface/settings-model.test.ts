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
      description: "Declared in catalog/games.yaml",
    })
    expect(games?.items.map(item => [item.label, item.value])).toEqual([
      ["On this device", "1 game"],
      ["Folder scan", "Ready"],
      ["Rescan game folders", "2 folders"],
      ["GBA", "Registered"],
      ["More games", "Registered"],
    ])
    expect(games?.items[3]?.interaction).toMatchObject({
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
})
