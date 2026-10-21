import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost } from "../src/fixtures/fixture-host"
import {
  checkingStatus,
  chooseSaveStatus,
  chooseSaveStressStatus,
  historyModel,
  historyStates,
  inUseStatus,
  markedGames,
  noSplitGames,
  savesModel,
} from "../src/fixtures/save-sync-fixtures"
import {
  formatSaveDuration,
  formatSaveEndedAt,
  formatSaveEndedAtInSentence,
  formatSavePlayed,
} from "../src/save-sync-labels"
import { ShiftSurface } from "../src/ShiftSurface"

afterEach(() => cleanup())

const local = (y: number, m: number, d: number, h = 0, min = 0) =>
  new Date(y, m - 1, d, h, min).getTime()

describe("save sync labels", () => {
  const now = new Date(2026, 9, 21, 16, 24)

  test("states the end time by day, in the device's local time", () => {
    expect(formatSaveEndedAt(local(2026, 10, 21, 9, 5), now, "en-US")).toBe("Today at 9:05 AM")
    expect(formatSaveEndedAt(local(2026, 10, 20, 21, 3), now, "en-US")).toBe("Yesterday at 9:03 PM")
    expect(formatSaveEndedAt(local(2026, 10, 17, 21, 3), now, "en-US")).toBe("Saturday at 9:03 PM")
    expect(formatSaveEndedAt(local(2026, 8, 2, 7, 0), now, "en-US")).toBe("Aug 2 at 7:00 AM")
    expect(formatSaveEndedAt(local(2025, 8, 2, 7, 0), now, "en-US")).toBe("Aug 2, 2025")
  })

  test("a clock ahead of this device is stated as a date, never as the future", () => {
    expect(formatSaveEndedAt(local(2026, 10, 21, 23, 0), now, "en-US")).toBe("Today at 11:00 PM")
    expect(formatSaveEndedAt(local(2026, 10, 25, 8, 0), now, "en-US")).toBe("Oct 25 at 8:00 AM")
    expect(formatSaveEndedAt(Number.NaN, now, "en-US")).toBe("Unknown time")
  })

  test("words the time inside a sentence", () => {
    expect(formatSaveEndedAtInSentence(local(2026, 10, 21, 9, 5), now, "en-US")).toBe("today at 9:05 AM")
    expect(formatSaveEndedAtInSentence(local(2026, 10, 17, 21, 3), now, "en-US")).toBe("on Saturday at 9:03 PM")
  })

  test("words a session length from zero to very long", () => {
    expect(formatSaveDuration(0)).toBe("Under a minute")
    expect(formatSaveDuration(-5)).toBe("Under a minute")
    expect(formatSaveDuration(59)).toBe("Under a minute")
    expect(formatSaveDuration(60)).toBe("1 min")
    expect(formatSaveDuration(3600)).toBe("1 h")
    expect(formatSaveDuration(72 * 60)).toBe("1 h 12 min")
    expect(formatSaveDuration(1204 * 3600 + 300, "en-US")).toBe("1,204 h 5 min")
    expect(formatSavePlayed(12)).toBe("Played under a minute")
    expect(formatSavePlayed(25 * 60)).toBe("Played 25 min")
  })
})

describe("split choice (ChooseSave)", () => {
  test("shows one card per side and picks with that side's own host action id", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={savesModel({ status: chooseSaveStatus() })} />)
    expect(screen.getByText("Saves split between two devices")).toBeTruthy()
    expect(screen.getByRole("heading", { name: "Wario Land 4" })).toBeTruthy()
    const group = screen.getByRole("group", { name: "Saves to continue from" })
    const cards = within(group).getAllByRole("button")
    expect(cards).toHaveLength(2)
    expect(cards[0]?.textContent).toContain("usu")
    expect(cards[0]?.textContent).toContain("Played 1 h 12 min")
    expect(cards[1]?.textContent).toContain("zao")
    expect(cards[1]?.textContent).toContain("Yesterday at")
    // The first card takes the cursor, so one confirm picks a side the
    // person is looking at.
    expect(document.activeElement === cards[0]).toBe(true)
    fireEvent.click(cards[1]!)
    expect(host.calls).toEqual(["action:saves-pick:sha256:bb22"])
  })

  test("Back runs the host's one action; with several, Back does nothing", () => {
    const host = createFixtureHost()
    const { rerender } = render(
      <ShiftSurface host={host} model={savesModel({ status: chooseSaveStatus() })} />,
    )
    act(() => host.press("back"))
    expect(host.calls).toEqual(["action:saves-choose-cancel"])

    rerender(<ShiftSurface host={host} model={savesModel({ status: chooseSaveStressStatus() })} />)
    act(() => host.press("back"))
    expect(host.calls).toEqual(["action:saves-choose-cancel"])
    fireEvent.click(screen.getByRole("button", { name: "Open save history" }))
    expect(host.calls.at(-1)).toBe("action:saves-choose-history")
  })

  test("states the host's detail and says when a side has no picture", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={savesModel({ status: chooseSaveStressStatus() })} />)
    expect(
      screen.getByText("No other device answered. Your pick syncs when your devices meet again."),
    ).toBeTruthy()
    expect(screen.getAllByText("No screenshot").length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText("Retroid-Pocket-Mini-V2-living-room-handheld")).toBeTruthy()
  })

  test("owns the screen over the rail and over Library", () => {
    const host = createFixtureHost()
    const { rerender } = render(<ShiftSurface host={host} model={savesModel()} />)
    fireEvent.click(screen.getByRole("button", { name: "Library" }))
    fireEvent.click(screen.getByRole("button", { name: "Library" }))
    rerender(<ShiftSurface host={host} model={savesModel({ status: chooseSaveStatus() })} />)
    expect(screen.queryByRole("button", { name: "Library" })).toBeNull()
    expect(screen.getByRole("group", { name: "Saves to continue from" })).toBeTruthy()
  })
})

describe("questions and checks before launch", () => {
  test("Busy checking other devices offers the host's Skip", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={savesModel({ status: checkingStatus })} />)
    expect(screen.getByText("Checking other devices…")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Skip" }))
    expect(host.calls).toEqual(["action:saves-check-skip"])
  })

  test("Confirm states the host's question and offers exactly its actions", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={savesModel({ status: inUseStatus })} />)
    // Main's Confirm on the rail (#115): the kicker, the focused answer in
    // full as the heading, and the host's detail.
    expect(screen.getByText("In use on zao since 4:02 PM")).toBeTruthy()
    expect(screen.getByRole("heading", { name: "Launch anyway" })).toBeTruthy()
    expect(screen.getByText(/may still be running there/)).toBeTruthy()
    const launch = screen.getByRole("button", { name: "Launch anyway" })
    expect(document.activeElement === launch).toBe(true)
    expect(screen.queryByRole("button", { name: "Wario Land 4, saves split" })).toBeNull()
    const cancel = screen.getByRole("button", { name: "Cancel" })
    fireEvent.focus(cancel)
    fireEvent.click(cancel)
    expect(host.calls).toEqual(["action:saves-inuse-cancel"])
  })
})

describe("save marks and the split notice", () => {
  const playedModel = (): SurfaceModel =>
    savesModel({
      catalog: {
        _tag: "Ready",
        games: markedGames().map((game, i) => ({ ...game, lastPlayedAt: Date.now() - (i + 1) * 3_600_000 })),
      },
    })

  test("Home states a standing split notice with the game's name", () => {
    render(<ShiftSurface host={createFixtureHost()} model={savesModel()} />)
    const notice = screen.getAllByRole("status").find(el => el.className === "shift-save-notice")
    expect(notice?.textContent).toContain("Wario Land 4")
    expect(notice?.textContent).toContain("Saves split between two devices")
    expect(screen.getByRole("button", { name: "Wario Land 4, saves split" })).toBeTruthy()
  })

  test("the notice gives way to launch work", () => {
    const { container } = render(
      <ShiftSurface host={createFixtureHost()} model={savesModel({ status: checkingStatus })} />,
    )
    expect(container.querySelector(".shift-save-notice")).toBeNull()
  })

  test("the focused game's hero states each of the five marks", () => {
    const host = createFixtureHost()
    const { container } = render(<ShiftSurface host={host} model={playedModel()} />)
    const hero = () => container.querySelector(".shift-cine-hero:last-child")?.textContent ?? ""
    expect(hero()).toContain("Saves split between two devices")
    const cases: readonly [string, string][] = [
      ["The Legend of Zelda: The Minish Cap", "Saves synced"],
      ["Advance Wars", "Not checked with other devices"],
      ["Neverball", "Saves not synced"],
      ["Hades", "Synced by Steam"],
    ]
    for (const [title, mark] of cases) {
      fireEvent.focus(screen.getByRole("button", { name: title }))
      expect(hero()).toContain(mark)
    }
  })

  test("Library and detail carry the split", () => {
    render(<ShiftSurface host={createFixtureHost()} model={savesModel()} />)
    fireEvent.click(screen.getByRole("button", { name: "Library" }))
    fireEvent.click(screen.getByRole("button", { name: "Library" }))
    const tile = screen.getByRole("button", { name: "Wario Land 4, saves split" })
    expect(tile.textContent).toContain("Split")
    fireEvent.click(tile)
    expect(screen.getByText("Saves split between two devices")).toBeTruthy()
  })

  test("the gameplay overlay states a split found during the session", () => {
    render(
      <ShiftSurface
        host={createFixtureHost()}
        model={savesModel({
          presentation: { kind: "gameplay-overlay", title: "Wario Land 4", controls: [], groups: [] },
          status: { _tag: "Running", kicker: "Playing", gameId: "local-game:wl4" },
        })}
      />,
    )
    expect(screen.getByText("Saves split between two devices")).toBeTruthy()
    expect(screen.getByText("Korri asks which save to keep before the next launch.")).toBeTruthy()
  })
})

describe("split review", () => {
  const openSaves = () => {
    const saves = screen.getByRole("button", { name: "Saves" })
    fireEvent.focus(saves)
    fireEvent.click(saves)
  }

  test("is reachable only when Korri states save marks", () => {
    render(<ShiftSurface host={createFixtureHost()} model={{ ...savesModel(), catalog: { _tag: "Ready", games: [{ id: "g", title: "G" }] } }} />)
    expect(screen.queryByRole("button", { name: "Saves" })).toBeNull()
  })

  test("lists split games and runs the mark's own choose action, not a launch", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={savesModel()} />)
    openSaves()
    expect(screen.getByRole("heading", { name: "Split saves" })).toBeTruthy()
    const rows = document.querySelectorAll("[data-save-index]")
    expect(rows).toHaveLength(1)
    fireEvent.click(rows[0]!)
    expect(host.calls).toEqual(["game-action:local-game:wl4:saves-choose:local-game:wl4"])
  })

  test("says all saves match when nothing is split, and Back leaves", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={savesModel({ catalog: { _tag: "Ready", games: noSplitGames() } })} />)
    openSaves()
    expect(screen.getByRole("heading", { name: "All saves match" })).toBeTruthy()
    act(() => host.press("back"))
    expect(screen.getByRole("button", { name: "Library" })).toBeTruthy()
    expect(host.calls).toEqual([])
  })
})

describe("save history", () => {
  const states = historyStates()

  test("loading, error and empty keep the head and the host's close action", () => {
    const host = createFixtureHost()
    const { rerender } = render(<ShiftSurface host={host} model={historyModel(states.loading)} />)
    expect(screen.getByText("Reading the save history…")).toBeTruthy()
    rerender(<ShiftSurface host={host} model={historyModel(states.error)} />)
    expect(screen.getByText("Korri could not read the save history")).toBeTruthy()
    expect(screen.getByText(/could not be read. Restart/)).toBeTruthy()
    rerender(<ShiftSurface host={host} model={historyModel(states.empty)} />)
    expect(screen.getByText("No saves yet")).toBeTruthy()
    act(() => host.press("back"))
    expect(host.calls).toEqual(["action:saves-history-close"])
    fireEvent.click(screen.getByRole("button", { name: "Close" }))
    expect(host.calls).toEqual(["action:saves-history-close", "action:saves-history-close"])
  })

  test("keeps Korri's order and marks current and interrupted saves", () => {
    render(<ShiftSurface host={createFixtureHost()} model={historyModel(states.ready)} />)
    const rows = Array.from(document.querySelectorAll("[data-save-index]"))
    expect(rows.map(row => row.querySelector(".shift-save-row-label")?.textContent)).toEqual([
      "usu", "zao", "usu", "Retroid-Pocket-Mini-V2-living-room-handheld", "zao",
    ])
    expect(rows[0]?.textContent).toContain("Current")
    expect(rows[0]?.tagName).toBe("DIV")
    expect(rows[1]?.textContent).toContain("Interrupted")
    expect(rows[4]?.textContent).toContain("Its files are on zao, which is not reachable.")
    expect(rows[4]?.tagName).toBe("DIV")
  })

  test("restore asks first, Cancel first and focused, and sends only on yes", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={historyModel(states.ready)} />)
    const rows = document.querySelectorAll<HTMLElement>("[data-save-index]")
    fireEvent.click(rows[1]!)
    const dialog = screen.getByRole("dialog", { name: "Restore this save?" })
    expect(within(dialog).getByText(/makes a new save from the one zao made yesterday at/)).toBeTruthy()
    const buttons = within(dialog).getAllByRole("button").filter(b => b.className === "shift-sheet-action")
    expect(buttons.map(b => b.textContent)).toEqual(["Cancel", "Restore"])
    expect(document.activeElement === buttons[0]).toBe(true)
    // Back closes the question, not the history.
    act(() => host.press("back"))
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(host.calls).toEqual([])
    fireEvent.click(rows[1]!)
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Restore" }))
    expect(host.calls).toEqual(["action:saves-restore:sha256:e02"])
  })
})
