import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import {
  launchLayerIndex,
  launchRowId,
  launchScopeIndex,
  launchSettingsFixtures as launch,
  withLaunchSettings,
} from "../src/fixtures/launch-settings"
import { PicoGameDetail } from "../src/ui/organisms/PicoGameDetail"
import { picoDetailViewFromGame } from "../src/pico-detail-view"
import { PicoSurface } from "../src/PicoSurface"
import { PicoSegments } from "../src/ui/atoms/PicoSegments"
import { initialNavigation } from "../src/state/navigation"
import { update } from "../src/state/update"
import type { PicoMessage } from "../src/state/messages"

afterEach(cleanup)

const ready = withLaunchSettings(fixtureModel, launch.ready)

function story(model: SurfaceModel, ...messages: PicoMessage[]) {
  let nav = update(initialNavigation(), { _tag: "KorriPublished" }, model).model
  let requests: readonly unknown[] = []
  for (const message of messages) {
    const next = update(nav, message, model)
    nav = next.model
    requests = next.requests
  }
  return { nav, requests }
}

describe("launch settings navigation", () => {
  test("opening starts on the first layer with no question; closing forgets it", () => {
    const { nav } = story(ready)
    expect(nav.launch).toEqual({ group: 0, scope: 0, question: { _tag: "None" } })
    const closed = update(nav, { _tag: "KorriPublished" }, fixtureModel).model
    expect(closed.launch).toBeUndefined()
  })

  test("Back closes the keyboard first, then asks Korri to close the editor", () => {
    const opened = story(ready, { _tag: "OpenedEditor", settingId: "launch:profile:new" })
    expect(opened.nav.launch?.question._tag).toBe("EditingText")
    const back = update(opened.nav, { _tag: "PressedBack" }, ready)
    expect(back.model.launch?.question._tag).toBe("None")
    expect(back.requests).toEqual([])
    expect(update(back.model, { _tag: "PressedBack" }, ready).requests).toEqual([{ _tag: "RunAction", actionId: "launch:cancel" }])
  })

  test("System, Options and Menu change nothing under the editor", () => {
    for (const tag of ["PressedSystem", "PressedOptions", "PressedMenu"] as const) {
      const { nav, requests } = story(ready, { _tag: tag })
      expect(requests).toEqual([])
      expect(nav.settings).toBeUndefined()
      expect(nav.find).toBe(false)
    }
  })

  test("a spine, a choice and a typed name go to the editor's layer and to Korri unchanged", () => {
    const game = launchLayerIndex(launch.ready, "game:wario")
    const chose = story(ready, { _tag: "ChoseSettingsGroup", group: game })
    expect(chose.nav.launch?.group).toBe(game)
    expect(chose.nav.settings).toBeUndefined()
    const cpuMax = launchRowId("game:wario/any", "cpu.max")
    expect(story(ready, { _tag: "ChangedSetting", settingId: cpuMax, value: "45" }).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: cpuMax, value: "45" }])
    const typed = story(ready, { _tag: "OpenedEditor", settingId: "launch:profile:new" },
      ...[..."Race"].map(character => ({ _tag: "TypedText", character }) as PicoMessage), { _tag: "SavedText" })
    expect(typed.requests).toEqual([{ _tag: "ChangeSetting", settingId: "launch:profile:new", value: "Race" }])
  })

  test("the keyboard closes once Korri has saved the name", () => {
    const saving: SurfaceModel = { ...ready, settingsStatus: { _tag: "Saving", settingId: "launch:profile:new" } }
    const opened = story(ready, { _tag: "OpenedEditor", settingId: "launch:profile:new" }).nav
    const during = update(opened, { _tag: "KorriPublished" }, saving).model
    expect(during.launch?.question).toMatchObject({ _tag: "EditingText", sawSaving: true })
    expect(update(during, { _tag: "KorriPublished" }, ready).model.launch?.question._tag).toBe("None")
  })

  test("only actions Korri publishes run", () => {
    expect(story(ready, { _tag: "PressedLaunchAction", actionId: "launch:reload" }).requests)
      .toEqual([{ _tag: "RunAction", actionId: "launch:reload" }])
    expect(story(ready, { _tag: "PressedLaunchAction", actionId: "launch:retry" }).requests).toEqual([])
  })

  test("deleting a profile asks first, then runs Korri's action", () => {
    const confirmation = { title: "Delete Battery saver?", message: "Its values go.", confirmLabel: "Delete profile", destructive: true }
    const asked = story(ready, { _tag: "AskedSettingConfirmation", actionId: "launch:profile:remove:battery", confirmation })
    expect(asked.requests).toEqual([])
    expect(update(asked.nav, { _tag: "ConfirmedSettingAction" }, ready).requests)
      .toEqual([{ _tag: "RunAction", actionId: "launch:profile:remove:battery" }])
  })
})

describe("launch settings screen", () => {
  test("covers the catalog, starts the cursor on the first row and shows each row's source and marks", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={ready} />)
    expect(screen.queryByRole("button", { name: /Celeste Classic/ })).toBeNull()
    expect(screen.getByText("LAUNCH SETTINGS")).toBeTruthy()
    expect(document.activeElement?.textContent).toContain("Runner")
    const row = screen.getByRole("button", { name: /CPU max/ }).closest("li")
    if (row === null) throw new Error("no CPU max row")
    expect(within(row).getByText("61 % (1.10 GHz) from Profile Battery saver")).toBeTruthy()
    expect(within(row).getByText("DEVICE-ONLY")).toBeTruthy()
    expect(within(row).getByText("Applies at next launch. Sets the device's clock limits.")).toBeTruthy()
    expect(row.textContent).toContain("◀ 89 % (1.61 GHz) ▶")
  })

  test("a press steps a percent to the next real clock step", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={ready} />)
    fireEvent.click(screen.getByRole("button", { name: /CPU max/ }))
    expect(host.calls).toEqual([`setting:${launchRowId("device/any", "cpu.max")}:100`])
  })

  test("Back asks Korri to close", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={ready} />)
    act(() => host.press("back"))
    expect(host.calls).toEqual(["action:launch:cancel"])
  })

  test("a conflict shows Korri's words with Reload and Close instead of the rows", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withLaunchSettings(fixtureModel, launch.conflict)} />)
    expect(screen.getByText("CHANGED ELSEWHERE")).toBeTruthy()
    expect(screen.getByText(/changed elsewhere\. Reload/)).toBeTruthy()
    expect(screen.queryByRole("button", { name: /CPU max/ })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "RELOAD" }))
    expect(host.calls).toEqual(["action:launch:reload"])
  })

  test("an error offers Retry; loading offers Close", () => {
    const host = createFixtureHost()
    const view = render(<PicoSurface host={host} model={withLaunchSettings(fixtureModel, launch.error)} />)
    expect(screen.getByText("Korri could not read launch settings.")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "RETRY" }))
    view.rerender(<PicoSurface host={host} model={withLaunchSettings(fixtureModel, launch.loading)} />)
    expect(screen.getByText("READING LAUNCH SETTINGS")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "CLOSE" }))
    expect(host.calls).toEqual(["action:launch:retry", "action:launch:cancel"])
  })

  test("without device facts the rows say the clock steps are unknown", () => {
    render(<PicoSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.factsMissing)} />)
    expect(screen.getAllByText("Applies at next launch. Sets the device's clock limits. Clock steps unknown.").length).toBeGreaterThan(0)
  })

  test("missing route layers keep the rest editable, say why, and offer the runner chooser", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withLaunchSettings(fixtureModel, launch.needsRunner)} />)
    expect(screen.getByText(/appear once you choose a runner\. Values in force leave them out\./)).toBeTruthy()
    expect(screen.getAllByRole("tab").map(tab => tab.textContent)).toEqual(["DEVICE", "GAME", "PROFILE BATTERY SAVER"])
    // One place per layer: no scope tabs to choose between.
    expect(screen.queryByRole("tablist", { name: "Runner" })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "CHOOSE A RUNNER" }))
    expect(host.calls).toEqual(["action:launch:runner"])
  })

  test("a device with no GPU clocks shows one read-only GPU line", () => {
    render(<PicoSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.noGpu)} />)
    expect(screen.getByText("GPU limits aren't available on this device")).toBeTruthy()
    expect(screen.queryByRole("button", { name: /GPU min/ })).toBeNull()
  })
})

describe("a choice marked by value", () => {
  const quiet = (selected: string) => withLaunchSettings(fixtureModel, {
    ...launch.ready,
    places: launch.ready.places.map(place => place.id !== "game:wario/any" ? place : {
      ...place,
      groups: [{
        title: "Profile",
        items: [{
          id: "launch:profile:pick",
          label: "Profile at next launch",
          value: "Quiet",
          interaction: {
            kind: "choice" as const,
            selected,
            choices: [{ value: "", label: "No profile" }, { value: "p1", label: "Quiet" }, { value: "p2", label: "Quiet" }],
          },
        }],
      }],
    }),
  })

  test("steps from the selected value, not from the first matching label", () => {
    const host = createFixtureHost()
    const model = quiet("p2")
    const game = launchLayerIndex(model.launchSettings ?? launch.ready, "game:wario")
    render(<PicoSurface host={host} model={model} />)
    fireEvent.click(screen.getAllByRole("tab")[game]!)
    fireEvent.click(screen.getByRole("button", { name: /Profile at next launch/ }))
    expect(host.calls).toEqual(["setting:launch:profile:pick:"])
  })
})

describe("inert actions", () => {
  test("a disabled Reload and Choose a runner are drawn dimmed, marked disabled, and do nothing", () => {
    const host = createFixtureHost()
    const editor = { ...launch.needsRunner, actions: launch.needsRunner.actions.map(action => action.id === "launch:cancel" ? action : { ...action, enabled: false }) }
    render(<PicoSurface host={host} model={withLaunchSettings(fixtureModel, editor)} />)
    for (const name of ["RELOAD", "CHOOSE A RUNNER"]) {
      const button = screen.getByRole("button", { name }) as HTMLButtonElement
      expect(button.disabled).toBe(true)
      fireEvent.click(button)
    }
    expect(host.calls).toEqual([])
  })
})

test("segments with a repeated label are keyed by value: no duplicate-key warning, one segment each", () => {
  const errors: unknown[] = []
  const original = console.error
  console.error = (...args: unknown[]) => { errors.push(args.join(" ")) }
  try {
    const { container } = render(
      <PicoSegments current={2} options={[{ value: "", label: "No profile" }, { value: "p1", label: "Quiet" }, { value: "p2", label: "Quiet" }]} />,
    )
    expect(container.querySelectorAll(".pico-segments-item")).toHaveLength(3)
    expect(container.querySelectorAll("[data-lit]")[0]?.textContent).toBe("QUIET")
    expect(errors.filter(error => String(error).includes("same key"))).toEqual([])
  } finally {
    console.error = original
  }
})

describe("places: layer by runner scope", () => {
  const mgba = launchScopeIndex(launch.ready, "game:wario", "mGBA")
  const game = launchLayerIndex(launch.ready, "game:wario")
  const MGBA = "game:wario/runner:retroarch/mgba"

  test("a layer's card shows a tab per runner scope; the scope holds across layers", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={ready} />)
    const scopes = screen.getByRole("tablist", { name: "Runner" })
    expect(within(scopes).getAllByRole("tab").map(tab => tab.textContent)).toEqual(["ANY RUNNER", "FAMILY RETROARCH", "MGBA", "GPSP"])
    fireEvent.click(within(scopes).getByRole("tab", { name: "MGBA" }))
    expect(screen.getAllByText("from Device for mGBA").length).toBeGreaterThan(0)
    fireEvent.click(screen.getAllByRole("tab", { name: "GAME" })[0]!)
    expect(within(screen.getByRole("tablist", { name: "Runner" })).getByRole("tab", { name: "MGBA" }).getAttribute("aria-selected")).toBe("true")
    expect(screen.getByRole("button", { name: /Frameskip/ })).toBeTruthy()
  })

  test("Add setting opens the runner's settings to choose from, and sends the one chosen", () => {
    const opened = story(ready, { _tag: "ChoseSettingsGroup", group: game }, { _tag: "ChoseLaunchScope", scope: mgba },
      { _tag: "OpenedPicker", settingId: launchRowId(MGBA, "add-setting") })
    expect(opened.nav.launch?.question).toEqual({ _tag: "PickingChoice", settingId: launchRowId(MGBA, "add-setting") })
    // Only a value Korri offers goes to Korri.
    expect(update(opened.nav, { _tag: "PickedChoice", value: "bios" }, ready).requests).toEqual([])
    const picked = update(opened.nav, { _tag: "PickedChoice", value: "lang" }, ready)
    expect(picked.requests).toEqual([{ _tag: "ChangeSetting", settingId: launchRowId(MGBA, "add-setting"), value: "lang" }])
    expect(picked.model.launch?.question._tag).toBe("None")
    expect(update(opened.nav, { _tag: "PressedBack" }, ready).model.launch?.question._tag).toBe("None")

    const host = createFixtureHost()
    render(<PicoSurface host={host} model={ready} />)
    fireEvent.click(screen.getAllByRole("tab", { name: "GAME" })[0]!)
    fireEvent.click(within(screen.getByRole("tablist", { name: "Runner" })).getByRole("tab", { name: "MGBA" }))
    fireEvent.click(screen.getByRole("button", { name: /Add setting/ }))
    const dialog = screen.getByRole("dialog", { name: "Add setting" })
    expect(within(dialog).getAllByRole("button").map(button => button.textContent)).toEqual(["Colour correction", "Language", "CANCEL"])
    expect(document.activeElement?.textContent).toBe("Colour correction")
    fireEvent.click(within(dialog).getByRole("button", { name: "Language" }))
    expect(host.calls).toEqual([`setting:${launchRowId(MGBA, "add-setting")}:lang`])
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  test("a number opens the keyboard holding its value, saves only within range, and INHERIT clears it", () => {
    const frameskip = launchRowId(MGBA, "setting:frameskip")
    const base = [{ _tag: "ChoseSettingsGroup", group: game }, { _tag: "ChoseLaunchScope", scope: mgba }, { _tag: "OpenedEditor", settingId: frameskip }] as PicoMessage[]
    const opened = story(ready, ...base)
    expect(opened.nav.launch?.question).toMatchObject({ _tag: "EditingText", draft: { text: "2" } })
    const tooMany = story(ready, ...base, { _tag: "TypedText", character: "4" }, { _tag: "SavedText" })
    expect(tooMany.requests).toEqual([])
    const fits = story(ready, ...base, { _tag: "PressedTextBackspace" }, { _tag: "TypedText", character: "7" }, { _tag: "SavedText" })
    expect(fits.requests).toEqual([{ _tag: "ChangeSetting", settingId: frameskip, value: "7" }])
    const cleared = story(ready, ...base, { _tag: "AskedClear" }, { _tag: "ConfirmedClear" })
    expect(cleared.requests).toEqual([{ _tag: "ChangeSetting", settingId: frameskip, value: "" }])
  })

  test("values only files set are statements the cursor reaches, with their source and scope", () => {
    render(<PicoSurface host={createFixtureHost()} model={ready} />)
    fireEvent.click(screen.getAllByRole("tab", { name: "GAME" })[0]!)
    const fact = screen.getByRole("group", { name: /Environment SDL_AUDIODRIVER/ })
    expect(fact.tabIndex).toBe(0)
    const row = fact.closest("li")
    if (row === null) throw new Error("no row")
    expect(within(row).getByText("from Game")).toBeTruthy()
    expect(within(row).getByText("DEVICE-ONLY")).toBeTruthy()
    expect(within(row).getByText("Set in library.yaml. Change it there.")).toBeTruthy()
    expect(screen.getByText("FROM FILES")).toBeTruthy()
    expect(screen.queryByRole("button", { name: /Environment/ })).toBeNull()
  })

  test("a runner that isn't installed and a runner whose facts are unavailable each get a note", () => {
    const view = render(<PicoSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.runnerUnavailable)} />)
    expect(screen.getByText(/names runner retroarch\/vba-m, which isn't installed/)).toBeTruthy()
    view.rerender(<PicoSurface host={createFixtureHost()} model={withLaunchSettings(fixtureModel, launch.factsUnavailable)} />)
    expect(screen.getByText("gpSP facts are unavailable: launch.prepare timed out after 2 s.")).toBeTruthy()
    fireEvent.click(screen.getAllByRole("tab", { name: "GAME" })[0]!)
    fireEvent.click(within(screen.getByRole("tablist", { name: "Runner" })).getByRole("tab", { name: "GPSP" }))
    expect(screen.getByRole("group", { name: /Runner facts unavailable/ })).toBeTruthy()
  })

  test("the game's page shows the profile its next launch uses beside Play", () => {
    const game = fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games[0]! : { id: "x", title: "x" }
    render(<PicoGameDetail actions={[]} game={picoDetailViewFromGame({ ...game, activeProfile: "Battery saver" })} onPlay={() => undefined} onRunAction={() => undefined} />)
    expect(screen.getByText("PROFILE")).toBeTruthy()
    expect(screen.getByText("Battery saver")).toBeTruthy()
  })
})

describe("launch modifiers", () => {
  const game = launchLayerIndex(launch.ready, "game:wario")
  const profile = launchLayerIndex(launch.ready, "profile:battery")

  test("each modifier is its own captioned group: on, off or Inherit, with where its value comes from", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withLaunchSettings(fixtureModel, launch.noProfilePicked)} />)
    fireEvent.click(screen.getAllByRole("tab")[game]!)
    expect(screen.getByText("MODIFIER GAMESCOPE")).toBeTruthy()
    const row = screen.getByRole("button", { name: /^Gamescope/ }).closest("li")
    if (row === null) throw new Error("no Gamescope row")
    expect(within(row).getByText("On from Device")).toBeTruthy()
    expect(within(row).getByText("DEVICE-ONLY")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: /^Gamescope/ }))
    expect(host.calls).toEqual([`setting:${launchRowId("game:wario/any", "modifier:korri/gamescope:enabled")}:true`])
  })

  test("a profile turning Gamescope off says so on its own layer", () => {
    render(<PicoSurface host={createFixtureHost()} model={ready} />)
    fireEvent.click(screen.getAllByRole("tab")[profile]!)
    const row = screen.getByRole("button", { name: /^Gamescope/ }).closest("li")
    expect(row?.textContent).toContain("from Profile Battery saver")
  })

  test("a modifier with no schema has its on/off row and no Add setting", () => {
    render(<PicoSurface host={createFixtureHost()} model={ready} />)
    fireEvent.click(screen.getAllByRole("tab")[game]!)
    expect(screen.getByRole("button", { name: /^MangoHud/ })).toBeTruthy()
    expect(screen.queryByRole("button", { name: /Add MangoHud setting/ })).toBeNull()
    expect(screen.getByRole("button", { name: /Add Gamescope setting/ })).toBeTruthy()
  })
})

test("each place shows only its own hooks, its routine first, marked in force or not", () => {
  render(<PicoSurface host={createFixtureHost()} model={ready} />)
  const hooks = () => screen.queryAllByRole("group", { name: /Before launch/ }).map(fact => fact.closest("li")?.textContent ?? "")
  // The Device: its routine, then its own step.
  expect(hooks()).toHaveLength(2)
  expect(hooks()[0]).toContain("In force: routine night-mode, used by Device")
  expect(hooks()[0]).toContain("Set in device.yaml. Change it there.")
  expect(hooks()[1]).toContain("In force: from Device")
  // The Game for any runner adds no hook, and repeats none of the Device's.
  fireEvent.click(screen.getAllByRole("tab")[launchLayerIndex(launch.ready, "game:wario")]!)
  expect(hooks()).toEqual([])
  fireEvent.click(within(screen.getByRole("tablist", { name: "Runner" })).getByRole("tab", { name: "GPSP" }))
  expect(hooks()).toHaveLength(1)
  expect(hooks()[0]).toContain("Not in force: mGBA runs this game")
})

test("Back closes Add setting's list and returns the cursor to the row that opened it; Korri hears nothing", () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={ready} />)
  fireEvent.click(screen.getAllByRole("tab")[launchLayerIndex(launch.ready, "game:wario")]!)
  fireEvent.click(within(screen.getByRole("tablist", { name: "Runner" })).getByRole("tab", { name: "MGBA" }))
  const opener = screen.getByRole("button", { name: /Add setting/ })
  opener.focus()
  fireEvent.click(opener)
  expect(screen.getByRole("dialog", { name: "Add setting" })).toBeTruthy()
  // A raw Escape is not Pico's to read: only the host's Back closes the list.
  fireEvent.keyDown(screen.getByRole("dialog", { name: "Add setting" }), { key: "Escape" })
  expect(screen.getByRole("dialog", { name: "Add setting" })).toBeTruthy()
  act(() => host.press("back"))
  expect(screen.queryByRole("dialog")).toBeNull()
  expect(document.activeElement).toBe(screen.getByRole("button", { name: /Add setting/ }))
  expect(host.calls).toEqual([])
})

describe("string settings", () => {
  const LANG = launchRowId("device/runner:retroarch/mgba", "setting:lang")
  const toMgba = [{ _tag: "ChoseLaunchScope", scope: launchScopeIndex(launch.ready, "device", "mGBA") }] as PicoMessage[]

  test("the editor starts from the raw text, and an emptied string saves as a value", () => {
    const opened = story(ready, ...toMgba, { _tag: "OpenedEditor", settingId: LANG })
    expect(opened.nav.launch?.question).toMatchObject({ _tag: "EditingText", draft: { text: "de" } })
    const emptied = story(ready, ...toMgba, { _tag: "OpenedEditor", settingId: LANG }, { _tag: "ClearedText" }, { _tag: "SavedText" })
    expect(emptied.requests).toEqual([{ _tag: "ChangeSetting", settingId: LANG, value: "" }])
    // A profile's name still needs one: Korri does not take an empty name.
    expect(story(ready, { _tag: "OpenedEditor", settingId: "launch:profile:new" }, { _tag: "SavedText" }).requests).toEqual([])
  })

  test("its Inherit row runs Korri's clear action", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={ready} />)
    fireEvent.click(within(screen.getByRole("tablist", { name: "Runner" })).getByRole("tab", { name: "MGBA" }))
    fireEvent.click(screen.getByRole("button", { name: /Inherit Language/ }))
    expect(host.calls).toEqual([`action:${launchRowId("device/runner:retroarch/mgba", "clear:setting:lang")}`])
  })
})
