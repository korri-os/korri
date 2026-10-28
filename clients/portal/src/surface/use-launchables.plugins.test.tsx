import { afterEach, describe, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { PluginRepositoryAddResult } from "@contracts/generated/korrid"
import { createInMemoryKorridClient } from "../korrid/client"
import {
  ADD_REPOSITORY_SETTING,
  inspectActionId,
  reviewActionId,
  toggleSettingId,
} from "./plugin-repositories"
import {
  RETRO,
  fakePluginClient,
  inspection,
  ok,
  repositories,
  retroCatalog,
} from "./plugin-repositories.fixtures"
import { settingsFrom } from "./settings-model"
import { useLaunchables, type Launchables } from "./use-launchables"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const roots: Root[] = []
afterEach(async () => {
  await act(async () => {
    for (const root of roots.splice(0)) root.unmount()
  })
})

const waitFor = async (predicate: () => boolean) => {
  for (let attempt = 0; attempt < 150; attempt += 1) {
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 10)) })
    if (predicate()) return
  }
  throw new Error("condition did not become true")
}

function mount(korrid: Parameters<typeof useLaunchables>[0]) {
  let current!: Launchables
  function Probe() {
    current = useLaunchables(korrid)
    return null
  }
  const root = createRoot(document.createElement("div"))
  roots.push(root)
  act(() => root.render(<Probe />))
  return () => current
}

const chiptune = { sourceUrl: RETRO, pluginId: "@ada:chiptune", releaseVersion: "1.0.0" }

describe("useLaunchables plugin rows", () => {
  test("the in-memory brain's plugin repositories reach Settings", async () => {
    const launchables = mount(createInMemoryKorridClient())
    await waitFor(() => launchables().facts.plugins?.repositories._tag === "Ready")
    expect(settingsFrom(launchables().facts).map(group => group.title)).toContain("Plugins")
  })

  test("reads repositories on load and routes plugin changes and actions to korrid", async () => {
    const { client, calls } = fakePluginClient({
      pluginRepositories: () => ok(repositories({ userAdded: [RETRO] })),
      pluginPublishers: () => ok({ entries: [] }),
      pluginCatalog: () => ok({ entries: retroCatalog }),
      inspectPlugin: () => ok(inspection),
      installPlugin: () => ok({}),
      setPluginEnabled: () => ok({}),
      addPluginRepository: () => ok(PluginRepositoryAddResult.AlreadyPresent),
    })
    const launchables = mount({ ...createInMemoryKorridClient(), ...client })
    await waitFor(() => launchables().facts.plugins?.catalogs[RETRO]?._tag === "Ready")
    expect(settingsFrom(launchables().facts).map(group => group.title)).toContain("github.com/ada/retro-plugins")

    act(() => launchables().changeSetting(ADD_REPOSITORY_SETTING, RETRO))
    await waitFor(() => launchables().settingsStatus._tag === "Problem")
    expect(launchables().settingsStatus).toEqual({ _tag: "Problem", settingId: ADD_REPOSITORY_SETTING, message: "Already added" })
    act(() => launchables().dismissSettingsProblem())

    act(() => launchables().runDeviceAction(inspectActionId(chiptune)))
    await waitFor(() => Object.keys(launchables().facts.plugins?.reviews ?? {}).length === 1)
    await waitFor(() => launchables().settingsStatus._tag === "Idle")
    act(() => launchables().runDeviceAction(reviewActionId(chiptune)))
    await waitFor(() => calls.some(call => call[0] === "setPluginEnabled"))
    expect(calls.filter(call => call[0] === "installPlugin")).toEqual([
      ["installPlugin", RETRO, "@ada:chiptune", "1.0.0", inspection.approval],
    ])

    await waitFor(() => launchables().settingsStatus._tag === "Idle")
    act(() => launchables().changeSetting(toggleSettingId("@ada:netplay"), "false"))
    await waitFor(() => calls.some(call => call[0] === "setPluginEnabled" && call[2] === false))
  })
})
