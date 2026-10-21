import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { launchLayerIndex, launchSettingsFixtures as launch, withLaunchSettings } from "../src/fixtures/launch-settings"
import { PicoSurface } from "../src/PicoSurface"
import { initialNavigation } from "../src/state/navigation"
import { update } from "../src/state/update"
import type { PicoMessage } from "../src/state/messages"

afterEach(cleanup)

const ready = withLaunchSettings(fixtureModel, launch.ready)

function story(model: SurfaceModel, ...messages: PicoMessage[]) {
  let nav = update(initialNavigation(), { _tag: "KorriPublished" }, model).model
  let requests: unknown[] = []
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
    expect(nav.launch).toEqual({ group: 0, question: { _tag: "None" } })
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
    const game = launchLayerIndex(launch.ready, "game")
    const chose = story(ready, { _tag: "ChoseSettingsGroup", group: game })
    expect(chose.nav.launch?.group).toBe(game)
    expect(chose.nav.settings).toBeUndefined()
    expect(story(ready, { _tag: "ChangedSetting", settingId: "launch:set:game:cpu.max", value: "45" }).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: "launch:set:game:cpu.max", value: "45" }])
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
    expect(document.activeElement?.textContent).toContain("CPU min")
    const row = screen.getByRole("button", { name: /CPU max/ }).closest("li")
    if (row === null) throw new Error("no CPU max row")
    expect(within(row).getByText("61 % (1.1 GHz) from Profile Battery saver")).toBeTruthy()
    expect(within(row).getByText("DEVICE-ONLY")).toBeTruthy()
    expect(within(row).getByText("Applies at next launch.")).toBeTruthy()
    expect(row.textContent).toContain("◀ 89 % (1.61 GHz) ▶")
  })

  test("a press steps a percent to the next real clock step", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={ready} />)
    fireEvent.click(screen.getByRole("button", { name: /CPU max/ }))
    expect(host.calls).toEqual(["setting:launch:set:device:cpu.max:100"])
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
    expect(screen.getAllByText("Applies at next launch. Clock steps unknown.").length).toBeGreaterThan(0)
  })
})
