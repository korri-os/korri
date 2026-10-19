/**
 * The download before a first launch (ADR 0012), through the treaty: the
 * question, the bar, the unpacking, the failure, and the size on a game.
 * Every assertion is about what the user sees or what Korri would be asked.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { picoBytesLabel, picoDownloadLabel } from "../src/pico-detail-view"
import { PicoSurface } from "../src/PicoSurface"
import { picoMeterCells } from "../src/ui/organisms/PicoLaunchStage"

afterEach(() => cleanup())

const withStatus = (status: SurfaceStatus): SurfaceModel => ({ ...fixtureModel, status })

const confirm: SurfaceStatus = {
  _tag: "Confirm",
  kicker: "Needs a download",
  detail: "FEX rootfs and Proton-CachyOS, 1.5 GB",
  gameId: "hollow",
  actions: [
    { id: "acquisition:1:download", label: "Download", enabled: true },
    { id: "acquisition:1:cancel", label: "Cancel", enabled: true },
  ],
}

describe("the question before a download", () => {
  test("is a yellow card naming the game, Korri's detail, and exactly Korri's actions", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withStatus(confirm)} />)

    const card = screen.getByRole("region", { name: "Needs a download" })
    expect(card.querySelector(".pico-card")?.getAttribute("data-tone")).toBe("ask")
    expect(screen.getByRole("heading", { name: "Hollow Knight" })).toBeTruthy()
    expect(screen.getByText("FEX rootfs and Proton-CachyOS, 1.5 GB")).toBeTruthy()
    expect(screen.getAllByRole("button").map(button => button.textContent)).toEqual(["Download", "Cancel"])
    // The shelf is not drawn under a question.
    expect(screen.queryByRole("button", { name: /Celeste Classic/ })).toBeNull()
  })

  test("reports the pressed action by Korri's id, and nothing for an inert one", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withStatus({
      ...confirm,
      actions: [confirm.actions[0]!, { ...confirm.actions[1]!, enabled: false }],
    })} />)

    fireEvent.click(screen.getByRole("button", { name: "Download" }))
    expect(host.calls).toEqual(["action:acquisition:1:download"])
    expect(screen.getByRole("button", { name: "Cancel" }).hasAttribute("disabled")).toBe(true)
  })

  test("without a known game or detail, the kicker is the heading and the card still asks", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withStatus({ ...confirm, detail: undefined, gameId: undefined })} />)
    expect(screen.getByRole("heading", { name: "Needs a download" })).toBeTruthy()
    expect(screen.getAllByRole("button")).toHaveLength(2)
  })

  test("host buttons other than those Korri published change nothing", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withStatus(confirm)} />)
    host.press("options")
    host.press("menu")
    host.press("system")
    expect(host.calls).toEqual([])
    expect(screen.getByRole("heading", { name: "Hollow Knight" })).toBeTruthy()
  })
})

describe("downloading", () => {
  const downloading = (done: number, total?: number, extra: Partial<Extract<SurfaceStatus, { _tag: "Busy" }>> = {}): SurfaceStatus => ({
    _tag: "Busy",
    kicker: "Downloading FEX rootfs",
    detail: "116 MB of 1.2 GB · 6.4 MB/s · 2:43 left",
    progress: { done, ...(total === undefined ? {} : { total }) },
    gameId: "hollow",
    actions: [{ id: "acquisition:2:cancel", label: "Cancel", enabled: true }],
    ...extra,
  })

  test("draws a real bar from Korri's bytes: whole sixteenths lit, the next one marked", () => {
    render(<PicoSurface host={createFixtureHost()} model={withStatus(downloading(116, 1160))} />)
    const bar = screen.getByRole("progressbar", { name: "Downloading FEX rootfs" })
    expect(bar.getAttribute("aria-valuenow")).toBe("10")
    expect(bar.getAttribute("data-meter")).toBe("progress")
    const cells = [...bar.querySelectorAll(".pico-launch-stage-cell")]
    expect(cells.filter(cell => cell.getAttribute("data-lit") === "on")).toHaveLength(1)
    expect(cells.filter(cell => cell.getAttribute("data-lit") === "next")).toHaveLength(1)
    expect(cells.filter(cell => cell.getAttribute("data-lit") === "off")).toHaveLength(14)
    expect(screen.getByText("116 MB of 1.2 GB · 6.4 MB/s · 2:43 left")).toBeTruthy()
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy()
  })

  test("at 90 % fourteen cells are lit; at the end all sixteen and nothing blinks", () => {
    expect(picoMeterCells({ done: 90, total: 100 })).toBe(14)
    expect(picoMeterCells({ done: 100, total: 100 })).toBe(16)
    expect(picoMeterCells({ done: 0, total: 100 })).toBe(0)
    expect(picoMeterCells({ done: 5, total: 0 })).toBe(0)
    expect(picoMeterCells({ done: 200, total: 100 })).toBe(16)
    const { container } = render(<PicoSurface host={createFixtureHost()} model={withStatus(downloading(100, 100))} />)
    expect(container.querySelectorAll('[data-lit="on"]')).toHaveLength(16)
    expect(container.querySelectorAll('[data-lit="next"]')).toHaveLength(0)
  })

  test("with a position but no end, the bar is indeterminate and claims no fraction", () => {
    render(<PicoSurface host={createFixtureHost()} model={withStatus(downloading(612, undefined, { detail: "612 MB so far · 6.4 MB/s" }))} />)
    const bar = screen.getByRole("progressbar", { name: "Downloading FEX rootfs" })
    expect(bar.getAttribute("data-meter")).toBe("indeterminate")
    expect(bar.hasAttribute("aria-valuenow")).toBe(false)
    expect(bar.querySelectorAll("[data-lit]")).toHaveLength(0)
    expect(screen.getByText("612 MB so far · 6.4 MB/s")).toBeTruthy()
  })

  test("Cancel reports Korri's action id; a stale or inert one sends nothing", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withStatus(downloading(116, 1160))} />)
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))
    expect(host.calls).toEqual(["action:acquisition:2:cancel"])
  })

  test("the game's cart is seated in the slot", () => {
    render(<PicoSurface host={createFixtureHost()} model={withStatus(downloading(116, 1160))} />)
    expect(screen.getByLabelText("Hollow Knight")).toBeTruthy()
  })
})

describe("unpacking", () => {
  const unpacking: SurfaceStatus = { _tag: "Busy", kicker: "Unpacking FEX rootfs", progress: { done: 0 }, gameId: "hollow" }

  test("shows the walking band, not a bar filled to the end, with no Cancel and no BACK hint", () => {
    const host = createFixtureHost()
    const { container } = render(<PicoSurface host={host} model={withStatus(unpacking)} />)
    expect(screen.getByRole("heading", { name: "Unpacking FEX rootfs" })).toBeTruthy()
    const meter = container.querySelector(".pico-launch-stage-meter")
    expect(meter?.getAttribute("data-meter")).toBe("indeterminate")
    expect(meter?.querySelectorAll(".pico-launch-stage-cell")).toHaveLength(16)
    // Nothing claims a fraction: no lit/unlit split, no value, no clock fill.
    expect(container.querySelectorAll("[data-lit]")).toHaveLength(0)
    expect(meter?.hasAttribute("aria-valuenow")).toBe(false)
    expect(container.querySelector('[data-meter="clock"]')).toBeNull()
    expect(screen.queryByRole("button")).toBeNull()
    expect(screen.queryByText("BACK")).toBeNull()
    host.press("back")
    expect(host.calls).toEqual([])
  })

  test("any Busy without actions drops the BACK hint; one with Cancel keeps it", () => {
    const { unmount } = render(<PicoSurface host={createFixtureHost()} model={withStatus({ _tag: "Busy", kicker: "Stopping session…", detail: "Waiting for the host to finish" })} />)
    expect(screen.queryByText("BACK")).toBeNull()
    unmount()
    render(<PicoSurface host={createFixtureHost()} model={withStatus({
      _tag: "Busy", kicker: "Downloading FEX rootfs", progress: { done: 1, total: 10 }, gameId: "hollow",
      actions: [{ id: "acquisition:2:cancel", label: "Cancel", enabled: true }],
    })} />)
    expect(screen.getByText("BACK")).toBeTruthy()
  })
})

describe("a failed download", () => {
  const failed: SurfaceStatus = {
    _tag: "Problem",
    kicker: "Couldn't download FEX rootfs",
    reason: "The connection stalled. Check the network and try again.",
    canRetry: true,
    gameId: "hollow",
    gameTitle: "Hollow Knight",
  }

  test("warns in red with Korri's words, against the game, and offers Retry and OK", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withStatus(failed)} />)
    const card = screen.getByRole("region", { name: "Couldn't download FEX rootfs" })
    expect(card.querySelector(".pico-card")?.getAttribute("data-tone")).toBe("warn")
    expect(screen.getByRole("heading", { name: "Hollow Knight" })).toBeTruthy()
    expect(screen.getByText("The connection stalled. Check the network and try again.")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "RETRY" }))
    fireEvent.click(screen.getByRole("button", { name: "CANCEL" }))
    expect(host.calls).toEqual(["retry", "dismiss"])
  })

  test("Back acknowledges it", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={withStatus(failed)} />)
    host.press("back")
    expect(host.calls).toEqual(["dismiss"])
  })
})

describe("a game that needs a download", () => {
  const needing = (): SurfaceModel => ({
    ...fixtureModel,
    catalog: fixtureModel.catalog._tag !== "Ready" ? fixtureModel.catalog : {
      _tag: "Ready",
      games: fixtureModel.catalog.games.map(game =>
        game.id === "celeste" ? { ...game, download: { bytes: 1_500_000_000 } }
          : game.id === "petal" ? { ...game, download: {} } : game),
    },
  })

  test("wears its size as a chip on the shelf stage, and only while it is chosen", () => {
    render(<PicoSurface host={createFixtureHost()} model={needing()} />)
    expect(screen.getByText("1.5 GB DOWNLOAD")).toBeTruthy()
    fireEvent.focus(screen.getByRole("button", { name: /Hollow Knight/ }))
    expect(screen.queryByText("1.5 GB DOWNLOAD")).toBeNull()
    fireEvent.focus(screen.getByRole("button", { name: /Petal Quest/ }))
    expect(screen.getByText("DOWNLOAD NEEDED")).toBeTruthy()
  })

  test("and on its own screen", () => {
    render(<PicoSurface host={createFixtureHost()} initialView={{ _tag: "Detail", gameId: "celeste" }} model={needing()} />)
    expect(screen.getByText("1.5 GB DOWNLOAD")).toBeTruthy()
    expect(screen.getByRole("button", { name: /▶ (PLAY|CONTINUE)/ })).toBeTruthy()
  })

  test("formats bytes the way the host's detail line does, and states nothing Korri did not", () => {
    expect(picoBytesLabel(1_500_000_000)).toBe("1.5 GB")
    expect(picoBytesLabel(1_160_000_000)).toBe("1.2 GB")
    expect(picoBytesLabel(2_000_000_000)).toBe("2 GB")
    expect(picoBytesLabel(612_000_000)).toBe("612 MB")
    expect(picoBytesLabel(6_400_000)).toBe("6.4 MB")
    expect(picoBytesLabel(48_000)).toBe("48 KB")
    expect(picoBytesLabel(900)).toBe("900 B")
    expect(picoDownloadLabel({})).toBeUndefined()
    expect(picoDownloadLabel({ download: {} })).toBe("DOWNLOAD NEEDED")
    expect(picoDownloadLabel({ download: { bytes: 0 } })).toBe("0 B DOWNLOAD")
  })
})
