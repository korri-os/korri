/**
 * Launch settings as one list (ADR 0066), through Pico's program as the
 * shell drives it, on the hand-written models of fixtures/launch-settings.ts:
 *
 *  - the view: each setting once under its section's caption, where the game
 *    runs, a changed row and its earlier value, a row that cannot change,
 *    the keep row, the unreachable holder, Loading, Conflict and no rows;
 *  - focus: up and down walk every row (the ones that cannot change too),
 *    the keep row and the floor; confirm changes a row as Settings does;
 *  - the scroll rule: the keep row stands outside the list's scroll, the
 *    list says when it holds more than it shows, and the list brings a row
 *    far out of sight back by its own word.
 *
 * That the focused control is drawn inside the window at every size is the
 * probe's to judge (focus-visible.test.ts walks these models).
 */
import { describe, expect, test } from "bun:test"
import { join } from "node:path"
import type { KorriModel, SurfaceLaunchSettings, SurfaceMessage, SurfaceRequest } from "../src/model"
import { picoLaunchViewFromModel } from "../src/pico-launch-view"
import { picoExits } from "../src/pico-display-plan"
import { launchWhereOnFloor, PICO_LAUNCH_READOUT_WIDTH, PICO_LAUNCH_SHORT } from "../src/native/panel"
import { catalogScreen, init, update, view } from "../src/native/program"
import { fixtureModel } from "./fixtures/fixture-host"
import { keepRow, launchFixtures, launchRowId, type LaunchFixture } from "./fixtures/launch-settings"
import { code, read, ROOT } from "./slint-source"

interface RowShown {
  section: string
  label: string
  value: string
  description: string
  source: string
  was: string
  wrap: boolean
  kind: string
  focused?: boolean
  focusId?: string
  deviceOnly: boolean
}
interface PanelShown {
  spines: unknown[]
  rows: RowShown[]
  cursor: number
  notes: string[]
  message: string
  where: string
  hasKeep: boolean
  keep: RowShown
}

function pico(fixture: LaunchFixture | SurfaceLaunchSettings, width = 640, height = 480) {
  const games = fixtureModel.catalog._tag === "Ready" ? [...fixtureModel.catalog.games] : []
  const launch = (value: LaunchFixture | SurfaceLaunchSettings) => (typeof value === "string" ? launchFixtures[value] : value)
  let model: KorriModel = {
    ...fixtureModel,
    catalog: { _tag: "Ready", total: games.length, window: games, firstIndex: 0, query: { text: "", order: "korri" } },
    launchSettings: launch(fixture),
  }
  let state = init(model, {})
  const requests: SurfaceRequest[] = []
  const send = (message: SurfaceMessage) => {
    const next = update(state, message, model)
    state = next.state
    requests.push(...next.requests)
  }
  send({ _tag: "Resized", width, height })
  send({ _tag: "KorriPublished" })
  requests.length = 0 // The catalog's own requests at the first publish are not the editor's.
  const shown = () => view(state, model) as unknown as { page: string; mode: string; panel: PanelShown; frame: { place: string; label: string; hasLead: boolean; lead: { label: string; focused?: boolean } }; card: { shown: boolean; kicker: string; title: string; message: string; actions: { label: string }[] }; focusPlan: Record<string, Record<string, { to: string; entry: string }>> }
  return {
    press: (...buttons: string[]) => buttons.forEach(button => send({ _tag: "Pressed", button } as SurfaceMessage)),
    tap: (target: string) => send({ _tag: "Activated", target }),
    publish: (next: LaunchFixture | SurfaceLaunchSettings) => {
      model = { ...model, launchSettings: launch(next) }
      send({ _tag: "KorriPublished" })
    },
    view: shown,
    panel: () => shown().panel,
    rows: () => shown().panel.rows,
    focus: () => state.focus,
    screen: () => catalogScreen(state, model),
    requests,
  }
}

const row = (section: string, label: string) => `row:${launchRowId(section, label)}`
const KEEP = "row:launch:keep"

describe("the one list", () => {
  test("each setting stands once in one card, the first of a section under its caption", () => {
    const page = pico("local")
    expect(page.panel().spines).toEqual([])
    expect(page.rows().map(shown => `${shown.section === "" ? "" : `# ${shown.section} `}${shown.label}: ${shown.value}`)).toEqual([
      "# VIDEO Render scale: 4x", "Aspect ratio: Fill view", "Window mode: Borderless", "FPS limit: Off",
      "# AUDIO Volume: 100%",
      "# PERFORMANCE CPU limit: 100%", "CPU governor: Powersave",
      "# RUNS WITH Runner: WiiCompiled", "Gamescope: On",
    ])
    const ids = page.rows().map(shown => shown.focusId)
    expect(new Set(ids).size).toBe(ids.length)
    expect(page.view()).toMatchObject({ page: "launch", mode: "panel", frame: { place: "Mario Kart Wii: Retro Rewind", label: "LAUNCH SETTINGS" } })
  })

  test("the card says where the game runs, in Korri's words; a short, wide screen says it on the floor row", () => {
    expect(pico("local").panel().where).toBe("RUNS HERE")
    expect(pico("streamed").panel().where).toBe("RUNS ON AKA")
    expect(pico("loading").panel().where).toBe("")
    const readout = (width: number, height: number) => (pico("streamed", width, height).view().frame as unknown as { readout: string }).readout
    const where = (width: number, height: number) => pico("streamed", width, height).panel().where
    // 1024x240: 512 by 120 virtual pixels. The floor has the width; the card keeps its lines for rows.
    expect([readout(1024, 240), where(1024, 240)]).toEqual(["RUNS ON AKA", ""])
    // 320x240 is as short but has no room on the floor; taller screens keep the chip in the card.
    expect([readout(320, 240), where(320, 240)]).toEqual(["", "RUNS ON AKA"])
    expect([readout(640, 480), where(640, 480)]).toEqual(["", "RUNS ON AKA"])
    expect([readout(480, 800), where(480, 800)]).toEqual(["", "RUNS ON AKA"])
    // One virtual pixel either side of each threshold.
    expect(launchWhereOnFloor({ width: PICO_LAUNCH_READOUT_WIDTH, height: PICO_LAUNCH_SHORT - 1 })).toBe(true)
    expect(launchWhereOnFloor({ width: PICO_LAUNCH_READOUT_WIDTH - 1, height: PICO_LAUNCH_SHORT - 1 })).toBe(false)
    expect(launchWhereOnFloor({ width: PICO_LAUNCH_READOUT_WIDTH, height: PICO_LAUNCH_SHORT })).toBe(false)
  })

  test("a streamed game shows the same list, with this device's Stream section in it", () => {
    const local = pico("local").rows().map(shown => shown.label)
    const streamed = pico("streamedClean").rows()
    expect(streamed.map(shown => shown.label).filter(label => local.includes(label))).toEqual(local)
    expect(streamed.filter(shown => !local.includes(shown.label)).map(shown => shown.label)).toEqual(["Stream size", "Stream frame rate", "Bitrate", "Codec"])
    expect(streamed.find(shown => shown.label === "Stream size")!.section).toBe("STREAM")
  })

  test("no changes: no row says an earlier value and there is no keep row", () => {
    const page = pico("local")
    expect(page.rows().some(shown => shown.was !== "")).toBe(false)
    expect(page.panel().hasKeep).toBe(false)
    expect(page.screen().targets.some(target => target.region === "keep")).toBe(false)
  })

  test("a changed row says its earlier value, focused or not; the others say none", () => {
    const page = pico("changes")
    expect(page.rows().filter(shown => shown.was !== "").map(shown => `${shown.label}: ${shown.value}, was ${shown.was}`)).toEqual([
      "Render scale: 2x, was 4x",
      "Aspect ratio: Original, was Fill view",
    ])
    page.press("down", "down")
    expect(page.rows().filter(shown => shown.was !== "").length).toBe(2)
  })

  test("a row set back to its earlier value loses its mark; with the last one the keep row goes", () => {
    const page = pico("changes")
    expect(page.panel().hasKeep).toBe(true)
    page.publish("local")
    expect(page.rows().some(shown => shown.was !== "")).toBe(false)
    expect(page.panel().hasKeep).toBe(false)
  })

  test("the keep row: Korri's words and the place in force, drawn apart from the list", () => {
    const page = pico("changes")
    expect(page.panel().keep).toMatchObject({ label: "Keep 2 changes", value: "This game", kind: "pick", focusId: KEEP })
    expect(page.rows().some(shown => shown.focusId === KEEP)).toBe(false)
    expect(pico("korriOnly").panel().keep.label).toBe("Keep 1 change")
    expect(pico("long").panel().keep).toMatchObject({ label: "Keep 7 changes", value: "This launch" })
  })

  test("Korri's keep row without a change is not drawn", () => {
    const stale = { ...launchFixtures.local, keep: keepRow(0) } as SurfaceLaunchSettings
    expect(pico(stale).panel().hasKeep).toBe(false)
  })

  test("only the focused row says where its value stays and what Korri says of it", () => {
    const page = pico("streamedClean")
    const noted = () => page.rows().filter(shown => shown.source !== "" || shown.description !== "").map(shown => `${shown.label}: ${shown.source}`)
    expect(noted()).toEqual(["Render scale: Stays on aka"])
    page.press("down", "down", "down", "down", "down")
    expect(noted()).toEqual(["Stream size: Stays on this device"])
    // Korri's `source` says where the value stays: no device-only mark says it twice.
    expect(page.rows().some(shown => shown.deviceOnly)).toBe(false)
  })

  test("a row that cannot change is a fact: it takes the cursor and says why there, and a press asks Korri for nothing", () => {
    const page = pico("local")
    const fps = () => page.rows().find(shown => shown.label === "FPS limit")!
    expect(fps()).toMatchObject({ kind: "fact", description: "", wrap: true })
    page.press("down", "down", "down")
    expect(page.focus()).toBe(row("Video", "FPS limit"))
    expect(fps()).toMatchObject({ focused: true, description: "Not supported by WiiCompiled" })
    page.press("confirm", "left", "right")
    expect(page.requests).toEqual([])
    const tapped = pico("local")
    tapped.tap(row("Video", "FPS limit"))
    expect(tapped.focus()).toBe(row("Video", "FPS limit"))
    expect(tapped.requests).toEqual([])
  })

  test("the unreachable holder: Korri's message in the card, RETRY on the floor under the cursor, its rows facts, this device's rows still change", () => {
    const page = pico("unreachable")
    expect(page.view()).toMatchObject({ mode: "panel", frame: { hasLead: true, lead: { label: "RETRY", focused: true } } })
    expect(page.panel().message).toBe("aka cannot be reached. Its settings cannot change now.")
    expect(page.panel().where).toBe("RUNS ON AKA")
    page.press("confirm")
    expect(page.requests).toEqual([{ _tag: "RunAction", actionId: "launch:retry" }])
    const stream = ["Stream size", "Stream frame rate", "Bitrate", "Codec"]
    expect(page.rows().filter(shown => shown.kind !== "fact").map(shown => shown.label)).toEqual(stream)
    // A holder's row says why while the cursor is on it.
    page.tap(row("Video", "Render scale"))
    expect(page.rows()[0]).toMatchObject({ focused: true, description: "aka cannot be reached.", source: "Stays on aka" })
    // This device's own row changes.
    page.requests.length = 0
    page.tap(row("Stream", "Codec"))
    expect(page.requests).toEqual([{ _tag: "ChangeSetting", settingId: launchRowId("Stream", "Codec"), value: "h-264" }])
    expect(page.panel().hasKeep).toBe(true)
  })

  test("only Korri's own rows: the list holds them and nothing else, never a fault", () => {
    const page = pico("korriOnly")
    expect(page.rows().filter(shown => shown.section !== "").map(shown => shown.section)).toEqual(["PERFORMANCE", "RUNS WITH"])
    expect(page.view().card.shown).toBe(false)
    expect(page.panel().message).toBe("")
  })

  test("no group at all is no fault: Pico says there is nothing to set, and RELOAD stays on the floor", () => {
    const page = pico("empty")
    expect(page.view().card).toMatchObject({ shown: true, kicker: "NOTHING TO SET", title: "Celeste Classic" })
    expect(page.view().frame).toMatchObject({ hasLead: true, lead: { label: "RELOAD", focused: true } })
  })

  test("Loading and Conflict keep their meaning; an error with no rows is Korri's words and its answers", () => {
    expect(pico("loading").view().card).toMatchObject({ shown: true, kicker: "READING" })
    const conflict = pico("conflict")
    expect(conflict.view().card).toMatchObject({ shown: true, kicker: "CHANGED ELSEWHERE", message: "Launch settings changed elsewhere. Reload to see them before you change them." })
    expect(conflict.view().card.actions.map(action => action.label)).toEqual(["RELOAD", "CLOSE"])
    expect(conflict.rows()).toEqual([])
    const error = pico("error")
    expect(error.view().card).toMatchObject({ shown: true, kicker: "COULD NOT READ", message: "Korri could not read launch settings." })
    expect(error.view().card.actions.map(action => action.label)).toEqual(["RETRY", "CLOSE"])
    error.press("confirm")
    expect(error.requests).toEqual([{ _tag: "RunAction", actionId: "launch:retry" }])
  })

  test("the old places are not read: a model that carries only places draws no row", () => {
    const local = launchFixtures.local as Extract<SurfaceLaunchSettings, { _tag: "Ready" }>
    const places = [{ id: "device/any", title: "Device", layer: { id: "device", title: "Device" }, scope: { kind: "any", label: "Any runner" }, groups: local.groups }]
    const old = { ...local, groups: undefined, changes: undefined, places } as unknown as SurfaceLaunchSettings
    const launch = picoLaunchViewFromModel({ ...fixtureModel, launchSettings: old })!
    expect(launch.rows).toEqual([])
    expect(launch.settings.groups).toEqual([])
  })
})

describe("focus", () => {
  test("the cursor starts on the first row; down walks every row, then the keep row, then the floor; up walks back", () => {
    const page = pico("changes")
    const seen = [page.focus()]
    for (let i = 0; i < 11; i++) {
      page.press("down")
      seen.push(page.focus())
    }
    const rows = [
      row("Video", "Render scale"), row("Video", "Aspect ratio"), row("Video", "Window mode"), row("Video", "FPS limit"), row("Audio", "Volume"),
      row("Performance", "CPU limit"), row("Performance", "CPU governor"), row("Runs with", "Runner"), row("Runs with", "Gamescope"),
    ]
    expect(seen).toEqual([...rows, KEEP, "lead", "lead"])
    const back: (string | undefined)[] = []
    for (let i = 0; i < 4; i++) {
      page.press("up")
      back.push(page.focus())
    }
    expect(back).toEqual([KEEP, row("Runs with", "Gamescope"), row("Runs with", "Runner"), row("Performance", "CPU governor")])
    expect(page.requests).toEqual([])
  })

  test("with no keep row the list goes straight to the floor and back to the row it held", () => {
    const page = pico("local")
    for (let i = 0; i < 9; i++) page.press("down")
    expect(page.focus()).toBe("lead")
    page.press("up")
    expect(page.focus()).toBe(row("Runs with", "Gamescope"))
  })

  test("confirm changes a choice row with its id, as on Settings", () => {
    const page = pico("changes")
    page.press("confirm")
    expect(page.requests).toEqual([{ _tag: "ChangeSetting", settingId: launchRowId("Video", "Render scale"), value: "3x" }])
  })

  test("the keep row opens Pico's list of the places on the one in force, and sends Korri the place chosen with the row's id", () => {
    for (const [width, height] of [[640, 480], [480, 800], [1024, 240], [1024, 768], [320, 240]] as const) {
      const page = pico("changes", width, height)
      page.tap(KEEP)
      expect(page.requests).toEqual([])
      expect(page.view().mode).toBe("choice")
      expect(page.screen().targets.map(target => target.id)).toEqual(["pick:launch", "pick:game", "pick:device", "pick:system:wii", "pick:release:usa", "pick:profile:battery", "cancel"])
      expect(page.focus()).toBe("pick:game")
      // The row's words are about the place in force: the list does not repeat them over every place.
      expect(page.view().card.message).toBe("")
      page.press("up", "confirm")
      expect(page.requests).toEqual([{ _tag: "ChangeSetting", settingId: "launch:keep", value: "launch" }])
      expect(page.view().mode).toBe("panel")
    }
  })

  test("another place, and cancel, on the same path", () => {
    const page = pico("changes")
    page.tap(KEEP)
    page.tap("pick:profile:battery")
    expect(page.requests).toEqual([{ _tag: "ChangeSetting", settingId: "launch:keep", value: "profile:battery" }])
    page.requests.length = 0
    page.tap(KEEP)
    page.tap("cancel")
    expect(page.requests).toEqual([])
    expect(page.view().mode).toBe("panel")
  })

  test("a keep target that is gone does nothing", () => {
    const page = pico("local")
    page.tap(KEEP)
    expect(page.requests).toEqual([])
    expect(page.view().mode).toBe("panel")
  })

  test("the floor's lead is Korri's first action that is not Close; back runs Close", () => {
    const page = pico("changes")
    page.tap("lead")
    expect(page.requests).toEqual([{ _tag: "RunAction", actionId: "launch:reload" }])
    page.requests.length = 0
    page.press("back")
    expect(page.requests).toEqual([{ _tag: "RunAction", actionId: "launch:cancel" }])
  })

  test("the plan's exits: rows, then the keep row, then the floor, at every size", () => {
    for (const beside of [true, false]) {
      const exits = picoExits(beside)
      expect(exits.rows!.down).toEqual([{ to: "keep", entry: "first" }, { to: "floor", entry: "first" }])
      expect(exits.keep).toEqual({ up: [{ to: "rows", entry: "remembered" }], down: [{ to: "floor", entry: "first" }] })
      expect(exits.floor!.up!.findIndex(exit => exit.to === "keep")).toBeLessThan(exits.floor!.up!.findIndex(exit => exit.to === "rows"))
      expect("scopes" in exits).toBe(false)
    }
    expect(pico("changes").view().focusPlan).toMatchObject({
      rows: { down: { to: "keep", entry: "first" } },
      keep: { up: { to: "rows", entry: "remembered" }, down: { to: "floor", entry: "first" } },
      floor: { up: { to: "keep", entry: "first" } },
    })
    expect(pico("local").view().focusPlan).toMatchObject({ rows: { down: { to: "floor", entry: "first" } }, floor: { up: { to: "rows", entry: "remembered" } } })
  })
})

describe("the scroll rule", () => {
  const source = (file: string) => code(read(join(ROOT, file)))

  test("the list names the row the cursor is on, so a row far out of sight comes back into it", () => {
    const page = pico("long")
    expect(page.panel().cursor).toBe(0)
    page.press("down", "down", "down")
    expect(page.panel().cursor).toBe(3)
    expect(page.rows()[3]!.focused).toBe(true)
    // On the keep row or the floor the cursor is on no row.
    page.tap(KEEP)
    page.press("back")
    for (let i = 0; i < 40; i++) page.press("down")
    expect(page.focus()).toBe("lead")
    expect(page.panel().cursor).toBe(-1)
    expect(source("pages/PicoSettingsPanel.slint")).toContain("cursor: panel.cursor == i;")
    expect(source("ui/molecules/PicoSettingRow.slint")).toContain("focused: (item.target && item.focused) || root.cursor;")
  })

  test("the keep row stands outside the scrolling body, in room the body gives up, so it never covers a row", () => {
    const page = source("pages/PicoSettings.slint")
    expect(page).toContain("pinned-room: root.keeping ? keep.height + Tokens.u : 0;")
    expect(page).toContain("y: shell.pinned-y;")
    const shell = source("ui/templates/PicoScreenShell.slint")
    expect(shell).toContain("body-h: max(0, floor.y - Tokens.u - body-y - pinned-room);")
    expect(shell).toContain("pinned-y: body-y + min(body-h, body.natural) + Tokens.u;")
  })

  test("launch settings' list says when it holds more than it shows, and the card says a title the bar had to cut", () => {
    const page = source("pages/PicoLaunchSettings.slint")
    expect(page).toContain("marks: true;")
    expect(page).toContain("says-place: true;")
    expect(source("ui/templates/PicoScreenShell.slint")).toContain("marks: root.marks;")
  })

  test("a focused launch row brings its lines into sight with its plate, and a control taller than the region shows its top", () => {
    expect(source("ui/molecules/PicoSettingRow.slint")).toContain("height: item.wrap ? root.height - plate.y : plate.height;")
    expect(source("ui/atoms/PicoScroll.slint")).toContain("want-tall > root.height - 2 * margin")
  })
})
