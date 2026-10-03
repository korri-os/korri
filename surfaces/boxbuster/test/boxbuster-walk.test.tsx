/**
 * Playing a game in Boxbuster: walk to a tape, pick it up, carry it to the
 * viewing room, and put it in the deck. There is no other way to start one.
 *
 * Driven the way the portal drives a surface: the host moves DOM focus and
 * turns confirm into a click; Back and Options arrive through `host.input`.
 * The store is drawn by a recording drawing, so the walk runs without WebGL
 * and the test can see what the scene was asked to show.
 */
import { afterEach, beforeEach, describe, expect, test } from "bun:test"
import type {
  SurfaceGame,
  SurfaceModel,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { BoxbusterSurface } from "../src/BoxbusterSurface"
import type {
  BoxbusterStoreDrawing,
  BoxbusterStoreProps,
} from "../src/BoxbusterStore"
import { createRecordingHost } from "./recording-host"

const DAY = 24 * 60 * 60 * 1000

/** A drawing that draws nothing and keeps what it was last asked to draw. */
function recordingDrawing(available = true) {
  const drawn: { last?: BoxbusterStoreProps } = {}
  const drawing: BoxbusterStoreDrawing = {
    available: () => available,
    Draw: props => {
      drawn.last = props
      return null
    },
  }
  return { drawing, drawn }
}

// happy-dom lays nothing out; give the surface a 640x480 handheld screen.
const realRect = HTMLElement.prototype.getBoundingClientRect
beforeEach(() => {
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    return this.hasAttribute("data-boxbuster-surface")
      ? new DOMRect(0, 0, 640, 480)
      : realRect.call(this)
  }
})
afterEach(() => {
  HTMLElement.prototype.getBoundingClientRect = realRect
  cleanup()
})

function modelWith(
  games: readonly SurfaceGame[],
  status: SurfaceStatus = { _tag: "Browsing" },
): SurfaceModel {
  return {
    presentation: { kind: "catalog" },
    catalog: { _tag: "Ready", games },
    status,
    actions: [],
    settings: [],
    settingsStatus: { _tag: "Idle" },
  }
}

const resumable: SurfaceGame = {
  id: "wario",
  title: "Wario Land 4",
  resumable: true,
  lastPlayedAt: Date.now() - DAY,
  playCount: 3,
}
const kirby: SurfaceGame = { id: "kirby", title: "Kirby" }
const skate: SurfaceGame = {
  id: "skate",
  title: "Skate 3",
  launchLocations: [
    { id: "copy-local", label: "This device" },
    { id: "copy-zao", label: "zao" },
  ],
}
/** Enough never-played games for the Classics shelves to need several steps. */
const classics = Array.from({ length: 60 }, (_, i) => ({
  id: `classic-${i}`,
  title: `Classic ${i}`,
}))

function focused(): HTMLElement {
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) throw new Error("nothing has focus")
  return active
}

const spotOf = (container: HTMLElement) =>
  container.querySelector("[data-boxbuster-surface]")?.getAttribute("data-spot")

const go = (label: string | RegExp) =>
  fireEvent.click(screen.getByRole("button", { name: label }))

describe("the fast path", () => {
  test("opens in the viewing room with the resumable tape in hand: one confirm plays it", () => {
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    const { container } = render(
      <BoxbusterSurface host={host} model={modelWith([kirby, resumable])} drawing={drawing} />,
    )
    expect(spotOf(container)).toBe("viewing")
    expect(drawn.last?.held).toEqual({
      tapeId: "wario",
      face: "front",
      pose: "carrying",
    })
    expect(focused().getAttribute("aria-label")).toBe(
      "Put Wario Land 4 in the deck",
    )
    // The room shows the deck has focus: its slot glows.
    expect(drawn.last?.focused).toBe("deck")
    fireEvent.click(focused())
    expect(host.calls).toEqual(["launch:wario:"])
    expect(drawn.last?.inDeck?.id).toBe("wario")
  })
})

describe("carrying a tape to the deck", () => {
  test("is the only way to play a game from the shelves", () => {
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    const { container } = render(
      <BoxbusterSurface host={host} model={modelWith([kirby])} drawing={drawing} />,
    )
    expect(spotOf(container)).toBe("door")
    act(() => go("Lobby"))
    act(() => go("Classics"))
    act(() => go("Shelf with Kirby"))
    expect(focused().getAttribute("aria-label")).toBe("Kirby")
    act(() => fireEvent.click(focused()))
    expect(drawn.last?.held?.tapeId).toBe("kirby")
    // Nothing on a shelf starts a game.
    expect(host.calls).toEqual([])
    act(() => go("Classics"))
    act(() => go("Lobby"))
    act(() => go("Viewing room"))
    expect(focused().getAttribute("aria-label")).toBe("Put Kirby in the deck")
    fireEvent.click(focused())
    expect(host.calls).toEqual(["launch:kirby:"])
  })

  test("asks which deck when Korri offers several places to play", () => {
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    render(
      <BoxbusterSurface
        host={host}
        model={modelWith([{ ...skate, resumable: true }])}
        drawing={drawing}
      />,
    )
    expect(drawn.last?.deckLabels).toEqual(["This device", "zao"])
    expect(
      screen.queryByRole("button", { name: "Put Skate 3 in the deck" }),
    ).toBeNull()
    go("Put Skate 3 in the zao deck")
    expect(host.calls).toEqual(["launch:skate:copy-zao"])
  })

  test("a loaded tape can be taken back out", () => {
    const { drawing, drawn } = recordingDrawing()
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([resumable])}
        drawing={drawing}
      />,
    )
    act(() => fireEvent.click(focused()))
    expect(focused().getAttribute("aria-label")).toBe("Take Wario Land 4 out")
    act(() => fireEvent.click(focused()))
    expect(drawn.last?.held?.tapeId).toBe("wario")
    expect(drawn.last?.inDeck).toBeUndefined()
  })
})

describe("a tape in hand", () => {
  test("Options turns it over, and Back puts it back on its shelf", () => {
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    render(<BoxbusterSurface host={host} model={modelWith([kirby])} drawing={drawing} />)
    act(() => go("Lobby"))
    act(() => go("Classics"))
    act(() => go("Shelf with Kirby"))
    act(() => fireEvent.click(focused()))
    // Picked up, it comes close enough to read the box.
    expect(drawn.last?.held).toEqual({
      tapeId: "kirby",
      face: "front",
      pose: "reading",
    })
    act(() => host.press("options"))
    expect(drawn.last?.held).toEqual({
      tapeId: "kirby",
      face: "back",
      pose: "reading",
    })
    act(() => host.press("back"))
    expect(drawn.last?.held).toBeUndefined()
    expect(focused().getAttribute("aria-label")).toBe("Kirby")
    expect(host.calls).toEqual([])
  })

  test("is read from the box itself: its rental sticker is printed on it", () => {
    const { drawing, drawn } = recordingDrawing()
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([resumable])}
        drawing={drawing}
      />,
    )
    const box = drawn.last?.placed.find(tape => tape.game.id === "wario")
    expect(box?.game.sticker).toBe("RENTED 3 TIMES")
  })
})

describe("the room", () => {
  test("has nothing laid over it: no text, only unseen focus targets", () => {
    const { container } = render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([resumable, kirby], {
          _tag: "Busy",
          kicker: "Starting…",
          detail: "Waking zao",
        })}
        drawing={recordingDrawing().drawing}
      />,
    )
    expect(container.textContent).toBe("")
    for (const button of screen.getAllByRole("button")) {
      expect(button.getAttribute("aria-label")).not.toBe("")
    }
  })

  test("shows which tape has focus", () => {
    const { drawing, drawn } = recordingDrawing()
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([kirby])}
        drawing={drawing}
      />,
    )
    act(() => go("Lobby"))
    act(() => go("Classics"))
    act(() => go("Shelf with Kirby"))
    expect(drawn.last?.focused).toBe("tape:kirby")
  })
})

describe("walking a shelf", () => {
  test("focusing the next stretch steps along it, landing on the nearest tape", () => {
    const { drawing } = recordingDrawing()
    const { container } = render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith(classics)}
        drawing={drawing}
      />,
    )
    act(() => go("Lobby"))
    act(() => go("Classics"))
    const shelf = screen.getAllByRole("button", { name: /^Shelf with / })[0]!
    act(() => fireEvent.click(shelf))
    const before = spotOf(container)
    act(() => screen.getByRole("button", { name: "Further along" }).focus())
    expect(spotOf(container)).not.toBe(before)
    expect(focused().getAttribute("data-kind")).toBe("Tape")
  })
})

describe("launch status on the TV", () => {
  test("a problem brings you to the TV, and one confirm retries", () => {
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    const { container, rerender } = render(
      <BoxbusterSurface host={host} model={modelWith([kirby])} drawing={drawing} />,
    )
    expect(spotOf(container)).toBe("door")
    rerender(
      <BoxbusterSurface
        host={host}
        model={modelWith([kirby], {
          _tag: "Problem",
          kicker: "Could not start",
          reason: "zao is asleep.",
          canRetry: true,
        })}
        drawing={drawing}
      />,
    )
    expect(spotOf(container)).toBe("viewing")
    // Korri's words are on the TV, and the deck is the retry button.
    expect(drawn.last?.tv).toMatchObject({ reason: "zao is asleep." })
    expect(focused().getAttribute("aria-label")).toBe("Try again")
    fireEvent.click(focused())
    expect(host.calls).toEqual(["retry"])
  })

  test("Back answers a problem before it puts a tape down", () => {
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    render(
      <BoxbusterSurface
        host={host}
        model={modelWith([resumable], {
          _tag: "Problem",
          kicker: "Could not start",
          reason: "The game stopped.",
          canRetry: false,
        })}
        drawing={drawing}
      />,
    )
    act(() => host.press("back"))
    expect(host.calls).toEqual(["dismiss"])
    expect(drawn.last?.held?.tapeId).toBe("wario")
  })

  test("Back on a failed launch ejects the tape and clears the problem", () => {
    const host = createRecordingHost()
    const { drawing, drawn } = recordingDrawing()
    const { rerender } = render(
      <BoxbusterSurface host={host} model={modelWith([resumable])} drawing={drawing} />,
    )
    act(() => fireEvent.click(focused()))
    expect(drawn.last?.inDeck?.id).toBe("wario")
    rerender(
      <BoxbusterSurface
        host={host}
        model={modelWith([resumable], {
          _tag: "Problem",
          kicker: "Could not start",
          reason: "The game stopped.",
          canRetry: true,
        })}
        drawing={drawing}
      />,
    )
    act(() => host.press("back"))
    expect(host.calls).toEqual(["launch:wario:", "dismiss"])
    expect(drawn.last?.inDeck).toBeUndefined()
    expect(drawn.last?.held?.tapeId).toBe("wario")
  })

  test("work under way goes to the TV", () => {
    const { drawing, drawn } = recordingDrawing()
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([resumable], {
          _tag: "Busy",
          kicker: "Starting…",
          detail: "Waking zao",
        })}
        drawing={drawing}
      />,
    )
    expect(drawn.last?.tv).toEqual({
      _tag: "Working",
      kicker: "Starting…",
      detail: "Waking zao",
    })
  })
})

describe("without WebGL", () => {
  test("the door says the store cannot open here, and nothing offers to play", () => {
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith([resumable])}
        drawing={recordingDrawing(false).drawing}
      />,
    )
    expect(screen.getByRole("heading").textContent).toBe(
      "The store cannot open on this device",
    )
    expect(screen.queryAllByRole("button")).toEqual([])
  })
})
