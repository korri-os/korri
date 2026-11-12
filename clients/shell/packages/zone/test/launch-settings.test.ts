/**
 * LAUNCH SETTINGS as one list (ADR 0066), through logic.ts as the shell
 * drives it, on the hand-written models of test/fixtures/launch-settings.ts:
 *
 *  - the view: each setting once under its section, where the game runs, a
 *    changed row and its earlier value, a row that cannot change, the keep
 *    row, the unreachable holder, Loading, Conflict, and a list with no rows;
 *  - focus: up and down walk every row (the ones that cannot change too),
 *    Korri's buttons and the keep row; left, right and confirm change a row;
 *  - the scroll rule: one row holds the list's place, and the keep row is
 *    no row of the list, so it never covers the focused row;
 *  - the plan: what fits is decided from the window, one px either side of
 *    each threshold.
 *
 * That the focused row is drawn inside the window at every size is judged
 * against the pixels by tests/zone_package.rs and the shell's probe.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceMessage, SurfaceModel, SurfaceRequest } from "../../../../../contracts/generated/surface"
import { init, update, view, type State } from "../logic"
import {
  launchBoxes,
  launchCursor,
  launchHead,
  launchPlan,
  launchRowFit,
  launchTitlePad,
  wasWords,
  ZONE_LAUNCH_ARROWS_ROOM,
  ZONE_LAUNCH_HEAD_MARGIN,
  ZONE_LAUNCH_LABEL_SHARE,
  ZONE_LAUNCH_TAG_PAD,
  ZONE_LAUNCH_TITLE_CAP,
  ZONE_LAUNCH_TITLE_CAP_LEAST,
  ZONE_LAUNCH_WAS_GAP,
  type ZoneLaunchOpen,
} from "../src/plan/launch-settings"
import { measureWords } from "../src/plan/measure"
import { zoneScreen } from "../src/plan/screen"
import { keepRow, launchFixtures, launchRowId, type LaunchFixture } from "./fixtures/launch-settings"
import { read, stripComments } from "./slint-source"

type Json = Record<string, unknown>
interface RowView {
  kind: string
  label: string
  value: string
  focused?: boolean
  focusId: string
  disabled: boolean
  choice: boolean
  changed: boolean
  was: string
  holds: boolean
  buttons: { label: string; focused?: boolean; focusId: string }[]
}

const BASE: Json = {
  presentation: { kind: "catalog" },
  catalog: { _tag: "Ready", total: 0, window: [], firstIndex: 0, query: { text: "", order: "korri" } },
  status: { _tag: "Browsing" },
  actions: [],
  settings: [],
  settingsStatus: { _tag: "Idle" },
}

const model = (fixture: LaunchFixture | ZoneLaunchOpen): SurfaceModel =>
  ({ ...BASE, launchSettings: typeof fixture === "string" ? launchFixtures[fixture] : fixture }) as unknown as SurfaceModel

/** The package as the shell runs it on one model; every request is kept. */
function zone(fixture: LaunchFixture | ZoneLaunchOpen, width = 640, height = 480) {
  let korri = model(fixture)
  let state: State = init(korri, {})
  const requests: SurfaceRequest[] = []
  const send = (message: SurfaceMessage) => {
    const next = update(state, message, korri)
    state = next.state
    requests.push(...next.requests)
  }
  send({ _tag: "Resized", width, height })
  send({ _tag: "KorriPublished" })
  const shown = () => view(state, korri) as unknown as Json
  const rows = () => shown().rows as RowView[]
  const keep = () => shown().keep as { shown: boolean; row: RowView; note: string }
  return {
    press: (...buttons: string[]) => buttons.forEach(button => send({ _tag: "Pressed", button } as SurfaceMessage)),
    tap: (target: string) => send({ _tag: "Activated", target }),
    publish: (next: LaunchFixture | ZoneLaunchOpen) => {
      korri = model(next)
      send({ _tag: "KorriPublished" })
    },
    view: shown,
    rows,
    keep,
    items: () => rows().filter(row => row.kind === "item"),
    /** The label of what holds the cursor: a row, a button, or the keep row. */
    focused: (): string | undefined => {
      const row = rows().find(entry => entry.focused === true)
      if (row !== undefined) return row.label
      const button = rows().flatMap(entry => entry.buttons).find(entry => entry.focused === true)
      if (button !== undefined) return button.label
      return keep().shown && keep().row.focused === true ? keep().row.label : undefined
    },
    /** The focused row's note: the row drawn under it. */
    note: (): string | undefined => {
      const all = rows()
      const at = all.findIndex(entry => entry.focused === true)
      return at >= 0 && all[at + 1]?.kind === "note" ? all[at + 1]!.label : undefined
    },
    requests,
  }
}

const open = (fixture: LaunchFixture) => launchFixtures[fixture] as ZoneLaunchOpen

describe("the one list", () => {
  test("each setting stands once, under its section, with the value the next launch would use", () => {
    const page = zone("local")
    const drawn = page.rows().filter(row => row.kind === "caption" || row.kind === "item").map(row => (row.kind === "caption" ? `# ${row.label}` : `${row.label}: ${row.value}`))
    expect(drawn).toEqual([
      "# Video", "Render scale: 4x", "Aspect ratio: Fill view", "Window mode: Borderless", "FPS limit: Off",
      "# Audio", "Volume: 100%",
      "# Performance", "CPU limit: 100%", "CPU governor: Powersave",
      "# Runs with", "Runner: WiiCompiled", "Gamescope: On",
    ])
    const ids = page.items().map(row => row.focusId)
    expect(new Set(ids).size).toBe(ids.length)
  })

  test("the head says where the game runs, in Korri's words", () => {
    const head = (fixture: LaunchFixture) => zone(fixture).view().head as { title: string; tags: { label: string; kind: string }[] }
    expect(head("local").tags).toEqual([{ label: "Runs here", kind: "system", korri: false }])
    expect(head("streamed").tags.map(tag => tag.label)).toEqual(["Runs on aka"])
    expect(head("streamed").title).toBe("Mario Kart Wii: Retro Rewind")
    // The unreachable holder: the same words, in the tag's trouble colour.
    expect(head("unreachable").tags).toEqual([{ label: "Runs on aka", kind: "remote", korri: false }])
  })

  test("a streamed game shows the same list, with this device's Stream section in it", () => {
    const local = zone("local").items().map(row => row.label)
    const streamed = zone("streamedClean").items().map(row => row.label)
    expect(streamed.filter(label => local.includes(label))).toEqual(local)
    expect(streamed.filter(label => !local.includes(label))).toEqual(["Stream size", "Stream frame rate", "Bitrate", "Codec"])
    expect(zone("streamedClean").rows().filter(row => row.kind === "caption").map(row => row.label)).toContain("Stream")
  })

  test("no changes: no row is marked and there is no keep row", () => {
    const page = zone("local")
    expect(page.items().some(row => row.changed)).toBe(false)
    expect(page.keep().shown).toBe(false)
    expect(page.keep().row.focusId).toBe("")
  })

  test("a changed row is marked and shows its earlier value: beside the value with room, in the focused row's note without", () => {
    // 1024x240: the value's box holds "2x" and "was 4x" side by side.
    const wide = zone("changes", 1024, 240)
    const scale = wide.items().find(row => row.label === "Render scale")!
    expect(scale).toMatchObject({ value: "2x", changed: true, was: "was 4x", focused: true })
    // One signal for one fact: the note does not say it again.
    expect(wide.note()).toBeUndefined()
    // 480x800: "Original" and "was Fill view" do not fit one box; the note says it while the cursor is there.
    const narrow = zone("changes", 480, 800)
    narrow.press("down")
    const aspect = narrow.items().find(row => row.label === "Aspect ratio")!
    expect(aspect).toMatchObject({ value: "Original", changed: true, was: "", focused: true })
    expect(narrow.note()).toBe("Was Fill view.")
    // The rows not changed carry no mark.
    expect(narrow.items().filter(row => row.changed).map(row => row.label)).toEqual(["Render scale", "Aspect ratio"])
  })

  test("a row set back to its earlier value loses its mark; with the last one the keep row goes", () => {
    const page = zone("changes")
    expect(page.keep().shown).toBe(true)
    page.publish("local")
    expect(page.items().some(row => row.changed)).toBe(false)
    expect(page.keep().shown).toBe(false)
  })

  test("the keep row: Korri's words, its place as the value, drawn apart from the list", () => {
    const page = zone("changes")
    expect(page.keep()).toMatchObject({ shown: true, row: { kind: "item", label: "Keep 2 changes", value: "This game", choice: true, focusId: "row:launch:keep" } })
    expect(page.rows().some(row => row.focusId === "row:launch:keep")).toBe(false)
    expect(zone("korriOnly").keep().row.label).toBe("Keep 1 change")
    expect(zone("long").keep().row).toMatchObject({ label: "Keep 7 changes", value: "This launch" })
  })

  test("Korri's keep row without a change is not drawn: it is present only while a row is changed", () => {
    const stale = { ...open("local"), keep: keepRow(0) } as ZoneLaunchOpen
    expect(zone(stale).keep().shown).toBe(false)
  })

  test("a row that cannot change is dimmed, still takes the cursor, and says why only while the cursor is on it", () => {
    const page = zone("local")
    const fps = () => page.items().find(row => row.label === "FPS limit")!
    expect(fps()).toMatchObject({ disabled: true, choice: false })
    expect(page.rows().some(row => row.kind === "note")).toBe(false)
    page.press("down", "down", "down")
    expect(page.focused()).toBe("FPS limit")
    expect(page.note()).toBe("Not supported by WiiCompiled.")
    // Left, right and confirm ask Korri for nothing.
    page.press("left", "right", "confirm")
    expect(page.requests).toEqual([])
    // A tap takes the cursor there too.
    const tapped = zone("local")
    tapped.tap(fps().focusId)
    expect(tapped.focused()).toBe("FPS limit")
    expect(tapped.requests).toEqual([])
  })

  test("a row's note says where its value stays", () => {
    const page = zone("streamedClean")
    expect(page.note()).toBe("Stays on aka.")
    page.press("down", "down", "down", "down", "down")
    expect(page.focused()).toBe("Stream size")
    expect(page.note()).toBe("Stays on this device.")
  })

  test("words a row had to cut are whole in its note", () => {
    const page = zone("long", 480, 800)
    page.press("down", "down", "down", "down", "down")
    expect(page.focused()).toBe("Texture filter")
    expect(page.note()).toBe("Sharp bilinear, integer prescale. Was Nearest. Stays on aka.")
    page.press("down")
    expect(page.focused()).toBe("Multisample anti-aliasing")
    expect(page.note()).toBe("Multisample anti-aliasing. Stays on aka.")
  })

  test("the unreachable holder: Korri's message and Retry first, its rows dimmed, this device's rows still change", () => {
    const page = zone("unreachable")
    const rows = page.rows()
    expect(rows[0]).toMatchObject({ kind: "problem", label: "aka cannot be reached. Its settings cannot change now." })
    expect(rows[1]!.kind).toBe("buttons")
    expect(rows[1]!.buttons.map(button => button.label)).toEqual(["Retry", "Close"])
    // The message asks for an action: the cursor starts on Retry.
    expect(page.focused()).toBe("Retry")
    page.press("confirm")
    expect(page.requests).toEqual([{ _tag: "RunAction", actionId: "launch:retry" }])
    const stream = ["Stream size", "Stream frame rate", "Bitrate", "Codec"]
    expect(page.items().filter(row => !row.disabled).map(row => row.label)).toEqual(stream)
    // A holder's row says why on the focused row only.
    page.press("down")
    expect(page.focused()).toBe("Render scale")
    expect(page.note()).toBe("aka cannot be reached. Stays on aka.")
    expect(page.rows().filter(row => row.kind === "note").length).toBe(1)
    // This device's own row changes.
    page.requests.length = 0
    page.press("down", "down", "down", "down", "down", "right")
    expect(page.focused()).toBe("Stream size")
    expect(page.requests).toEqual([{ _tag: "ChangeSetting", settingId: launchRowId("Stream", "Stream size"), value: "1920-1080" }])
    expect(page.keep().shown).toBe(true)
  })

  test("only Korri's own rows: the list holds them and nothing else, never a fault", () => {
    const page = zone("korriOnly")
    expect(page.rows().filter(row => row.kind === "caption").map(row => row.label)).toEqual(["Performance", "Runs with"])
    expect(page.rows().some(row => row.kind === "problem")).toBe(false)
  })

  test("no group at all is no fault: a word and Korri's buttons", () => {
    const page = zone("empty")
    expect(page.rows().map(row => row.kind)).toEqual(["word", "buttons"])
    expect(page.rows()[0]!.label).toBe("No settings")
    expect(page.focused()).toBe("Reload")
  })

  test("Loading and Conflict keep their meaning: Korri's word or message and its buttons, no rows", () => {
    const loading = zone("loading")
    expect(loading.rows().map(row => row.kind)).toEqual(["word", "buttons"])
    expect((loading.view().head as { tags: { label: string }[] }).tags.map(tag => tag.label)).toEqual(["Loading"])
    const conflict = zone("conflict")
    expect(conflict.rows().map(row => row.kind)).toEqual(["problem", "buttons"])
    expect(conflict.rows()[0]!.label).toBe("Launch settings changed elsewhere. Reload to see them before you change them.")
    expect(conflict.focused()).toBe("Reload")
    const error = zone("error")
    expect(error.rows().map(row => row.kind)).toEqual(["problem", "buttons"])
    expect(error.focused()).toBe("Retry")
  })

  test("the old places are not read: a model that carries only places draws no row", () => {
    const places = [{ id: "device/any", title: "Device", layer: { id: "device", title: "Device" }, scope: { kind: "any", label: "Any runner" }, groups: open("local").groups }]
    const old = { ...open("local"), groups: undefined, changes: undefined, places } as unknown as ZoneLaunchOpen
    expect(zone(old).items()).toEqual([])
  })
})

describe("focus", () => {
  test("down walks every row, then Korri's buttons, then the keep row; up walks back", () => {
    const page = zone("changes")
    const seen: (string | undefined)[] = [page.focused()]
    for (let i = 0; i < 12; i++) {
      page.press("down")
      seen.push(page.focused())
    }
    expect(seen).toEqual([
      "Render scale", "Aspect ratio", "Window mode", "FPS limit", "Volume", "CPU limit", "CPU governor", "Runner", "Gamescope",
      "Reload", "Keep 2 changes", "Keep 2 changes", "Keep 2 changes",
    ])
    const back: (string | undefined)[] = []
    for (let i = 0; i < 11; i++) {
      page.press("up")
      back.push(page.focused())
    }
    expect(back).toEqual(["Reload", "Gamescope", "Runner", "CPU governor", "CPU limit", "Volume", "FPS limit", "Window mode", "Aspect ratio", "Render scale", "Render scale"])
    expect(page.requests).toEqual([])
  })

  test("left and right walk Korri's buttons; the list is entered again on the row it held", () => {
    const page = zone("local")
    page.press("down", "down")
    for (let i = 0; i < 7; i++) page.press("down")
    expect(page.focused()).toBe("Reload")
    page.press("right")
    expect(page.focused()).toBe("Close")
    page.press("left", "up")
    expect(page.focused()).toBe("Gamescope")
  })

  test("under an error the buttons stand first: up from the first row reaches them, down from them the first row", () => {
    const page = zone("unreachable")
    expect(page.focused()).toBe("Retry")
    page.press("right")
    expect(page.focused()).toBe("Close")
    page.press("down")
    expect(page.focused()).toBe("Render scale")
    page.press("up")
    expect(page.focused()).toBe("Retry")
    // From the keep row, up goes to the list's last row.
    page.tap("row:launch:keep")
    expect(page.focused()).toBe("Keep 2 changes")
    page.requests.length = 0
    page.press("up")
    expect(page.focused()).toBe("Gamescope")
  })

  test("left, right and confirm change a choice row with its id; the keep row's id goes the same way", () => {
    const page = zone("changes")
    page.press("right")
    page.press("left")
    page.press("confirm")
    const scale = launchRowId("Video", "Render scale")
    expect(page.requests).toEqual([
      { _tag: "ChangeSetting", settingId: scale, value: "3x" },
      { _tag: "ChangeSetting", settingId: scale, value: "1x" },
      { _tag: "ChangeSetting", settingId: scale, value: "3x" },
    ])
    page.requests.length = 0
    page.tap("row:launch:keep")
    // A tap confirms: the place steps forward, from This game to Every game.
    expect(page.requests).toEqual([{ _tag: "ChangeSetting", settingId: "launch:keep", value: "device" }])
    page.requests.length = 0
    page.press("left", "right")
    expect(page.requests).toEqual([
      { _tag: "ChangeSetting", settingId: "launch:keep", value: "launch" },
      { _tag: "ChangeSetting", settingId: "launch:keep", value: "device" },
    ])
    expect(page.keep().note).toBe("Used every time you play this game.")
  })

  test("when the keep row goes while the cursor is on it, the cursor takes the list's last row", () => {
    const page = zone("changes")
    page.tap("row:launch:keep")
    page.publish("local")
    expect(page.focused()).toBe("Gamescope")
  })

  test("back runs Korri's close action", () => {
    const page = zone("changes")
    page.press("back")
    expect(page.requests).toEqual([{ _tag: "RunAction", actionId: "launch:cancel" }])
  })

  test("the regions and their exits follow the page's zones, top to bottom", () => {
    const plan = (fixture: LaunchFixture) => zone(fixture).view().focusPlan as Json
    expect(plan("changes")).toEqual({
      list: { down: { to: "actions", entry: "active" } },
      actions: { up: { to: "list", entry: "active" }, down: { to: "keep", entry: "active" } },
      keep: { up: { to: "actions", entry: "active" } },
    })
    expect(plan("unreachable")).toEqual({
      actions: { down: { to: "list", entry: "active" } },
      list: { up: { to: "actions", entry: "active" }, down: { to: "keep", entry: "active" } },
      keep: { up: { to: "list", entry: "active" } },
    })
    expect(plan("local")).toEqual({
      list: { down: { to: "actions", entry: "active" } },
      actions: { up: { to: "list", entry: "active" } },
    })
  })

  test("the row a region is entered on is marked: the first row from the buttons above, the last from the keep row", () => {
    const active = (page: ReturnType<typeof zone>) => (page.rows() as unknown as Json[]).filter(row => row.focusActive === true).map(row => row.label)
    const error = zone("unreachable")
    expect(active(error)).toEqual(["Render scale"])
    error.tap("row:launch:keep")
    expect(active(error)).toEqual(["Gamescope"])
  })
})

describe("the scroll rule", () => {
  test("in every state one target holds the list's place or the keep row holds the cursor, never both", () => {
    for (const fixture of ["local", "changes", "streamed", "unreachable", "long", "korriOnly"] as const) {
      const page = zone(fixture)
      for (let i = 0; i < 30; i++) {
        const holding = page.rows().filter(row => row.holds)
        const keep = page.keep()
        const onKeep = keep.shown && keep.row.focused === true
        expect(holding.length + (onKeep ? 1 : 0), `${fixture} after ${i} presses`).toBe(1)
        // The focused row's note is the row under it: the list brings both into view together.
        if (holding[0]?.kind === "item") expect(holding[0].focused).toBe(true)
        page.press("down")
      }
    }
  })

  test("the keep row is drawn outside the list (it never covers a row), and the list marks that more rows stand above or below", () => {
    const band = stripComments(read("ui/organisms/ZoneSkyBand.slint"))
    // The list's height gives the keep row its room.
    expect(band).toContain("height: Math.max(0, parent.height - 8 * root.a - parent.keep-h);")
    expect(band).toContain("if root.marks && list.more-above: ZoneShape")
    expect(band).toContain("if root.marks && list.more-below: ZoneShape")
    const page = stripComments(read("pages/ZoneLaunchSettings.slint"))
    expect(page).toContain("keep: root.keep;")
    expect(page).toContain("marks: true;")
  })
})

describe("the plan", () => {
  const screen = (width: number, height: number) => zoneScreen(width, height, "blend")

  test("the page and the plan agree on a row's boxes", () => {
    expect(stripComments(read("pages/ZoneLaunchSettings.slint"))).toContain(`label-share: ${ZONE_LAUNCH_LABEL_SHARE};`)
    const row = stripComments(read("ui/molecules/ZoneOptionRow.slint"))
    expect(row).toContain(`arrows-room: it.choice && it.kind == "item" ? ${ZONE_LAUNCH_ARROWS_ROOM} * a : 0;`)
    expect(row).toContain(`- now-w.preferred-width - ${ZONE_LAUNCH_WAS_GAP} * a - was-w.preferred-width`)
    // 640x480: 304 art px of list, 167 of label panel; the words' boxes in px.
    expect(launchBoxes(screen(640, 480), false)).toEqual({ label: 2 * (167 - 19), value: 2 * (304 - 167 - 4 - 26) })
    expect(launchBoxes(screen(640, 480), true).value).toBe(2 * (304 - 167 - 4 - 26 - 8))
  })

  test("the earlier value stands beside the value while both fit, one px either side", () => {
    const at = screen(640, 480)
    const box = launchBoxes(at, true).value
    const cap = 10 * at.a
    const needs = (value: string, was: string) => measureWords(value, cap, "fact", "blend") + ZONE_LAUNCH_WAS_GAP * at.a + measureWords(wasWords(was), cap, "fact", "blend")
    // Find a pair that fits exactly at some width, then move the window one px.
    const pair = needs("Original", "Fill view")
    const widths = Array.from({ length: 800 }, (_, i) => 480 + i)
    const least = widths.find(width => launchRowFit("Aspect ratio", false, "Original", "Fill view", true, zoneScreen(width, 480, "blend")).wasBeside)!
    expect(launchBoxes(zoneScreen(least, 480, "blend"), true).value).toBeGreaterThanOrEqual(pair)
    expect(launchRowFit("Aspect ratio", false, "Original", "Fill view", true, zoneScreen(least - 1, 480, "blend")).wasBeside).toBe(false)
    expect(launchRowFit("Render scale", false, "2x", "4x", true, at)).toEqual({ wasBeside: true, labelCut: false, valueCut: false })
    expect(needs("2x", "4x")).toBeLessThanOrEqual(box)
    // A row not changed has no earlier value to place.
    expect(launchRowFit("Render scale", false, "4x", undefined, true, at).wasBeside).toBe(false)
  })

  test("the longest label and value are cut where the window is narrow, whole where it is wide", () => {
    expect(launchRowFit("Multisample anti-aliasing", false, "Off", undefined, true, screen(480, 800)).labelCut).toBe(true)
    expect(launchRowFit("Multisample anti-aliasing", false, "Off", undefined, true, screen(1024, 768)).labelCut).toBe(false)
    expect(launchRowFit("Texture filter", false, "Sharp bilinear, integer prescale", undefined, true, screen(640, 480)).valueCut).toBe(true)
    expect(launchRowFit("Texture filter", false, "Sharp bilinear, integer prescale", undefined, true, screen(1024, 240)).valueCut).toBe(false)
  })

  test("the title keeps its size while it fits beside the tag, then yields size, and takes its own line where the window is tall", () => {
    const zelda = "The Legend of Zelda: Echoes of Wisdom"
    expect(launchHead("Wario Land 4", "Runs on aka", screen(640, 480))).toEqual({ cap: ZONE_LAUNCH_TITLE_CAP * 2, stacked: false })
    // Wide and short: the title shrinks beside its tag, never under the least.
    const strip = launchHead(zelda, "Runs on aka", screen(640, 480))
    expect(strip.stacked).toBe(false)
    expect(strip.cap).toBeLessThan(ZONE_LAUNCH_TITLE_CAP * 2)
    expect(strip.cap).toBeGreaterThanOrEqual(ZONE_LAUNCH_TITLE_CAP_LEAST * 2)
    // Tall: height pays, the tag goes under the title.
    expect(launchHead(zelda, "Runs on aka", screen(480, 800)).stacked).toBe(true)
    // No tag: nothing to stack.
    expect(launchHead(zelda, undefined, screen(480, 800))).toEqual({ cap: ZONE_LAUNCH_TITLE_CAP * 2, stacked: false })
    // The cap chosen is the largest whose words fit the room the band draws for them.
    for (const [width, height] of [[640, 480], [480, 800], [1024, 240], [1024, 768], [320, 240]] as const) {
      const at = screen(width, height)
      const head = launchHead(zelda, "Runs on aka", at)
      const tag = head.stacked ? 0 : measureWords("Runs on aka", 10 * at.a, "caps", "blend") + ZONE_LAUNCH_TAG_PAD * at.a
      const room = (cap: number) => width - ZONE_LAUNCH_HEAD_MARGIN * at.a - tag - 2 * launchTitlePad(cap) * at.a
      const fits = (cap: number) => measureWords(zelda, cap * at.a, "korri", "blend") <= room(cap)
      const cap = head.cap / at.a
      if (cap > ZONE_LAUNCH_TITLE_CAP_LEAST) expect(fits(cap), `${width}x${height} at ${cap}`).toBe(true)
      if (cap < ZONE_LAUNCH_TITLE_CAP) expect(fits(cap + 1), `${width}x${height} at ${cap + 1}`).toBe(false)
    }
  })

  test("the cursor starts on the first row, or on Korri's first button when the message asks for one or there is no row", () => {
    const plan = (fixture: LaunchFixture) => launchPlan(open(fixture), { _tag: "Idle" })
    expect(launchCursor(plan("local"), undefined, undefined)).toEqual({ zone: "list", rowId: launchRowId("Video", "Render scale") })
    expect(launchCursor(plan("unreachable"), undefined, undefined)).toEqual({ zone: "actions", actionId: "launch:retry" })
    expect(launchCursor(plan("loading"), undefined, undefined)).toEqual({ zone: "actions", actionId: "launch:cancel" })
    expect(plan("changes").zones).toEqual(["list", "actions", "keep"])
    expect(plan("unreachable").zones).toEqual(["actions", "list", "keep"])
    expect(plan("conflict").zones).toEqual(["actions"])
  })
})
