/**
 * The counter: where every decision in Boxbuster happens.
 *
 * Driven the way the portal drives a surface: the host moves DOM focus and
 * turns confirm into a click, and Back arrives through `host.input`. happy-dom
 * has no WebGL, so these tests also prove the fallback: with no store to draw,
 * the counter is the whole surface and still reaches every tape.
 */
import { afterEach, describe, expect, test } from "bun:test"
import type {
  SurfaceGame,
  SurfaceModel,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { BoxbusterSurface } from "../src/BoxbusterSurface"
import { createRecordingHost } from "./recording-host"

afterEach(cleanup)

const DAY = 24 * 60 * 60 * 1000

function modelWith(
  games: readonly SurfaceGame[] | "loading",
  status: SurfaceStatus = { _tag: "Browsing" },
): SurfaceModel {
  return {
    presentation: { kind: "catalog" },
    catalog: games === "loading" ? { _tag: "Loading" } : { _tag: "Ready", games },
    status,
    actions: [],
    settings: [],
    settingsStatus: { _tag: "Idle" },
  }
}

const resumable: SurfaceGame = {
  id: "wario",
  title: "Wario Land 4",
  subtitle: "GBA · This device",
  resumable: true,
  lastPlayedAt: Date.now() - DAY,
  playCount: 3,
  totalPlaytimeSeconds: 5400,
}
const recent: SurfaceGame = {
  id: "metroid",
  title: "Metroid Fusion",
  lastPlayedAt: Date.now() - 2 * DAY,
  playCount: 1,
  totalPlaytimeSeconds: 600,
}
const classic: SurfaceGame = { id: "kirby", title: "Kirby" }
const twoPlaces: SurfaceGame = {
  id: "skate",
  title: "Skate 3",
  launchLocations: [
    { id: "copy-local", label: "This device" },
    { id: "copy-zao", label: "zao" },
  ],
}

function focused(): HTMLElement {
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) throw new Error("nothing has focus")
  return active
}

describe("the fast path", () => {
  test("opens holding the tape you can resume, so one confirm resumes it", () => {
    const host = createRecordingHost()
    render(
      <BoxbusterSurface host={host} model={modelWith([classic, resumable])} />,
    )
    expect(focused().textContent).toBe("Resume")
    fireEvent.click(focused())
    expect(host.calls).toEqual(["launch:wario:"])
  })

  test("opens the same way when the catalog arrives after the surface", () => {
    const host = createRecordingHost()
    const { rerender } = render(
      <BoxbusterSurface host={host} model={modelWith("loading")} />,
    )
    rerender(<BoxbusterSurface host={host} model={modelWith([resumable])} />)
    expect(focused().textContent).toBe("Resume")
  })
})

describe("browsing the index", () => {
  test("lists every tape under the aisle the store shelves it in", () => {
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([classic, recent, resumable])}
      />,
    )
    const shelves = Array.from(
      document.querySelectorAll("nav ul[aria-labelledby]"),
    )
    const aisles = shelves.map(list => ({
      aisle: document.getElementById(
        list.getAttribute("aria-labelledby") ?? "",
      )?.textContent,
      tapes: Array.from(list.querySelectorAll("button")).map(
        button => button.textContent,
      ),
    }))
    expect(aisles).toEqual([
      { aisle: "Return cart", tapes: ["Wario Land 4"] },
      { aisle: "New releases", tapes: ["Metroid Fusion"] },
      { aisle: "Classics", tapes: ["Kirby"] },
    ])
  })

  test("with nothing to resume, stands in front of the first tape", () => {
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([recent, classic])}
      />,
    )
    expect(focused().textContent).toBe("Metroid Fusion")
  })

  test("shows the back of the box for the tape focus is on", () => {
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([recent, classic])}
      />,
    )
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe(
      "Metroid Fusion",
    )
    expect(screen.getByText("Played once")).toBeTruthy()
    act(() => screen.getByRole("button", { name: "Kirby" }).focus())
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Kirby")
  })

  test("confirm picks a tape up, and a second confirm plays it", () => {
    const host = createRecordingHost()
    render(<BoxbusterSurface host={host} model={modelWith([recent, classic])} />)
    fireEvent.click(screen.getByRole("button", { name: "Kirby" }))
    expect(focused().textContent).toBe("Play")
    fireEvent.click(focused())
    expect(host.calls).toEqual(["launch:kirby:"])
  })

  test("a held tape keeps directional focus at the counter", () => {
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([resumable, recent])}
      />,
    )
    const index = screen.getByRole("button", { name: "Metroid Fusion" })
    expect(index.closest("[inert]")).not.toBeNull()
  })
})

describe("putting a tape down", () => {
  test("Back puts it down and leaves focus on it in the index", () => {
    const host = createRecordingHost()
    render(<BoxbusterSurface host={host} model={modelWith([resumable, recent])} />)
    act(() => host.press("back"))
    expect(focused().textContent).toBe("Wario Land 4")
    expect(focused().closest("[inert]")).toBeNull()
    expect(host.calls).toEqual([])
  })

  test("a touch can put it down too", () => {
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([resumable, recent])}
      />,
    )
    fireEvent.click(screen.getByRole("button", { name: "Put it back" }))
    expect(focused().textContent).toBe("Wario Land 4")
  })
})

describe("choosing where to play", () => {
  test("asks which device rather than picking one", () => {
    const host = createRecordingHost()
    render(<BoxbusterSurface host={host} model={modelWith([twoPlaces])} />)
    fireEvent.click(screen.getByRole("button", { name: "Skate 3" }))
    expect(screen.queryByRole("button", { name: "Play" })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "Play on zao" }))
    expect(host.calls).toEqual(["launch:skate:copy-zao"])
  })
})

describe("launch status", () => {
  test("states the work under way", () => {
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([resumable], {
          _tag: "Busy",
          kicker: "Starting…",
          detail: "Waking zao",
          gameId: "wario",
        })}
      />,
    )
    expect(screen.getByText("Starting…")).toBeTruthy()
    expect(screen.getByText("Waking zao")).toBeTruthy()
  })

  test("a problem takes focus, and one confirm retries it", () => {
    const host = createRecordingHost()
    render(
      <BoxbusterSurface
        host={host}
        model={modelWith([resumable], {
          _tag: "Problem",
          kicker: "Could not start",
          reason: "zao is asleep.",
          canRetry: true,
          gameId: "wario",
          gameTitle: "Wario Land 4",
        })}
      />,
    )
    expect(screen.getByText("zao is asleep.")).toBeTruthy()
    expect(focused().textContent).toBe("Try again")
    fireEvent.click(focused())
    expect(host.calls).toEqual(["retry"])
  })

  test("Back clears a problem before it puts a tape down", () => {
    const host = createRecordingHost()
    render(
      <BoxbusterSurface
        host={host}
        model={modelWith([resumable], {
          _tag: "Problem",
          kicker: "Could not start",
          reason: "The game stopped.",
          canRetry: false,
        })}
      />,
    )
    expect(focused().textContent).toBe("Back to the shelves")
    act(() => host.press("back"))
    expect(host.calls).toEqual(["dismiss"])
  })
})

describe("without WebGL", () => {
  test("the counter is the whole surface and no store is drawn", () => {
    const { container } = render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([resumable])}
      />,
    )
    expect(container.querySelector("canvas")).toBeNull()
    expect(
      container.querySelector("[data-boxbuster-surface]")?.getAttribute(
        "data-layout",
      ),
    ).toBe("CounterOnly")
  })
})
