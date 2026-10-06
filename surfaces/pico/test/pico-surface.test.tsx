/**
 * Pico end to end, through the treaty and nothing else.
 *
 * Every assertion is about content the user would see or a command Korri would
 * receive. A test that only proved the surface mounted would pass against a
 * blank screen, which is the exact failure a fixture-fed surface produces when
 * its data never arrives.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceAction, SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const model = (overrides: Partial<SurfaceModel> = {}): SurfaceModel => ({
  ...fixtureModel,
  ...overrides,
})

describe("the shelf", () => {
  test("shows every game Korri published", () => {
    render(<PicoSurface host={createFixtureHost()} model={model()} />)

    expect(screen.getByRole("button", { name: /Celeste Classic/ })).toBeTruthy()
    expect(screen.getByRole("button", { name: /Hollow Knight/ })).toBeTruthy()
    expect(screen.getByRole("button", { name: /Tetris/ })).toBeTruthy()
    expect(screen.getByRole("button", { name: /Spelunky/ })).toBeTruthy()
  })

  test("states where a game came from alongside its title", () => {
    render(<PicoSurface host={createFixtureHost()} model={model()} />)

    expect(
      screen.getByRole("button", { name: "Celeste Classic, PICO-8 · This device" }),
    ).toBeTruthy()
  })

  test("stands in for missing cover art instead of leaving a hole", () => {
    render(<PicoSurface host={createFixtureHost()} model={model()} />)

    // Petal Quest is the one fixture game with no art: its cart carries a
    // sticker with its initials rather than an empty window.
    expect(screen.getByText("PQ")).toBeTruthy()
  })

  test("names the collection the focused game belongs to", () => {
    render(<PicoSurface host={createFixtureHost()} model={model()} />)
    // Korri grouped the first game under "Continue"; crossing into another
    // group is the only way a one-row shelf can show a boundary.
    expect(screen.getByText("CONTINUE")).toBeTruthy()
  })

  test("follows focus with the caption and the position", () => {
    render(<PicoSurface host={createFixtureHost()} model={model()} />)

    fireEvent.focus(screen.getByRole("button", { name: /Tetris/ }))

    expect(screen.getByRole("heading", { name: "Tetris" })).toBeTruthy()
    expect(screen.getByLabelText("3 of 9")).toBeTruthy()
  })

  test("shows the clock Korri preformatted", () => {
    render(<PicoSurface host={createFixtureHost()} model={model()} />)

    expect(screen.getByText("10:24")).toBeTruthy()
  })
})

describe("launching", () => {
  test("opens the game rather than launching it blind", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model()} />)

    fireEvent.click(screen.getByRole("button", { name: /Celeste Classic/ }))

    expect(host.calls).toEqual([])
    expect(screen.getByText("LIBRARY ›")).toBeTruthy()
    expect(screen.getByRole("heading", { name: "Celeste Classic" })).toBeTruthy()
  })
})

describe("a game's own screen", () => {
  const open = (name: RegExp) => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model()} />)
    fireEvent.click(screen.getByRole("button", { name }))
    return host
  }

  test("states what Korri knows about the game and nothing more", () => {
    open(/Hollow Knight/)

    expect(screen.getByText("Switch · This device")).toBeTruthy()
    expect(screen.getByText("3")).toBeTruthy()
    expect(screen.getByText("PLAYS")).toBeTruthy()
    expect(screen.getByText("2H 10M")).toBeTruthy()
    expect(screen.getByText("PLAYED")).toBeTruthy()
  })

  test("says never played instead of inventing a count", () => {
    open(/Celeste Classic/)

    expect(screen.getByText("NEVER PLAYED")).toBeTruthy()
    expect(screen.queryByText("PLAYS")).toBeNull()
  })

  test("offers CONTINUE when Korri says the game resumes", () => {
    open(/Hollow Knight/)
    expect(screen.getByRole("button", { name: /CONTINUE/ })).toBeTruthy()
  })

  test("offers PLAY when it does not", () => {
    open(/Celeste Classic/)
    expect(screen.getByRole("button", { name: /PLAY/ })).toBeTruthy()
    expect(screen.queryByRole("button", { name: /CONTINUE/ })).toBeNull()
  })

  test("launches directly when Korri offers no choice", () => {
    const host = open(/Celeste Classic/)

    fireEvent.click(screen.getByRole("button", { name: /PLAY/ }))

    expect(host.calls).toEqual(["launch:celeste"])
  })

  test("returns to the shelf on Back", () => {
    const host = open(/Celeste Classic/)

    act(() => host.press("back"))

    expect(screen.queryByText("LIBRARY ›")).toBeNull()
    expect(screen.getByText("LIBRARY")).toBeTruthy()
    expect(host.calls).toEqual([])
  })

  test("asks where to play rather than picking for the user", () => {
    const host = open(/Tetris/)
    fireEvent.click(screen.getByRole("button", { name: /PLAY/ }))

    // Nothing has been launched yet: the question is the whole point.
    expect(host.calls).toEqual([])
    expect(screen.getByText("PLAY WHERE?")).toBeTruthy()
    expect(screen.getByRole("button", { name: "This device" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "zao" })).toBeTruthy()
  })

  test("sends the location the user chose, unchanged", () => {
    const host = open(/Tetris/)
    fireEvent.click(screen.getByRole("button", { name: /PLAY/ }))
    fireEvent.click(screen.getByRole("button", { name: "zao" }))

    expect(host.calls).toEqual(["launch:tetris:zao"])
  })
})

describe("when there is no shelf to show", () => {
  test("says so while Korri is still reading the library", () => {
    render(
      <PicoSurface
        host={createFixtureHost()}
        model={model({ catalog: { _tag: "Loading" } })}
      />,
    )

    expect(screen.getByText("READING CARTS")).toBeTruthy()
  })

  test("explains an empty library without sounding broken", () => {
    render(
      <PicoSurface
        host={createFixtureHost()}
        model={model({ catalog: { _tag: "Empty" } })}
      />,
    )

    expect(screen.getByText("NO CARTS")).toBeTruthy()
  })

  test("shows Korri's failure copy and offers a way out", () => {
    const host = createFixtureHost()
    render(
      <PicoSurface
        host={host}
        model={model({
          catalog: { _tag: "Error", message: "The library folder is not readable." },
        })}
      />,
    )

    expect(screen.getByText("The library folder is not readable.")).toBeTruthy()

    fireEvent.click(screen.getByRole("button", { name: "TRY AGAIN" }))

    expect(host.calls).toEqual(["reload"])
  })
})

describe("while Korri is doing something with a game", () => {
  test("offers the host's real cancellation action during startup", () => {
    const host = createFixtureHost()
    render(
      <PicoSurface host={host} model={model({ status: {
        _tag: "Busy", kicker: "Starting Wario Land 4", gameId: "hollow",
        actions: [{ id: "cancel-launch", label: "Cancel", enabled: true }],
      } })} />,
    )
    const cancel = screen.getByRole("button", { name: "Cancel" })
    fireEvent.click(cancel)
    expect(host.calls).toEqual(["action:cancel-launch"])
    expect(screen.queryByRole("button", { name: /Celeste Classic/ })).toBeNull()
  })

  test("lets you cancel each pending launch by name and shows where it stands", () => {
    const host = createFixtureHost()
    render(
      <PicoSurface host={host} model={model({ status: {
        _tag: "Busy", kicker: "2 launches are starting",
        detail: "Cancel each launch you do not want.",
        actions: [
          { id: "cancel-pending:a", label: "Cancel Tetris", description: "Preparing", enabled: true },
          { id: "cancel-pending:b", label: "Cancel Spelunky", description: "Cancelling", enabled: false },
        ],
      } })} />,
    )
    const tetris = screen.getByRole("button", { name: /^Cancel Tetris/ })
    const spelunky = screen.getByRole("button", { name: /^Cancel Spelunky/ }) as HTMLButtonElement
    // Where each launch stands sits under its own row, as Pico's control notes do.
    expect(tetris.closest("li")?.textContent).toContain("Preparing")
    expect(spelunky.closest("li")?.textContent).toContain("Cancelling")
    expect(tetris.textContent).toBe("Cancel Tetris")
    expect(document.activeElement === tetris).toBe(true)
    expect(spelunky.disabled).toBe(true)
    fireEvent.click(spelunky)
    fireEvent.click(tetris)
    expect(host.calls).toEqual(["action:cancel-pending:a"])
  })

  test("puts the cursor back on a row when the focused launch leaves the list", () => {
    const host = createFixtureHost()
    const busy = (actions: SurfaceAction[]): SurfaceModel => model({ status: {
      _tag: "Busy", kicker: "2 launches are starting", actions,
    } })
    const tetris = { id: "cancel-pending:a", label: "Cancel Tetris", enabled: true }
    const spelunky = { id: "cancel-pending:b", label: "Cancel Spelunky", enabled: true }
    const { rerender } = render(<PicoSurface host={host} model={busy([tetris, spelunky])} />)
    const second = screen.getByRole("button", { name: "Cancel Spelunky" })
    second.focus()
    rerender(<PicoSurface host={host} model={busy([tetris, spelunky])} />)
    // Focus that is still on screen stays where the user put it.
    expect(document.activeElement === second).toBe(true)
    rerender(<PicoSurface host={host} model={busy([tetris])} />)
    // Without a cursor, the host's next confirm would press an unseen row.
    expect(document.activeElement === screen.getByRole("button", { name: "Cancel Tetris" })).toBe(true)
    expect(host.calls).toEqual([])
  })

  test("moves the cursor to the next row, not the first, when the pressed row is disabled", () => {
    const host = createFixtureHost()
    const busy = (actions: SurfaceAction[]): SurfaceModel => model({ status: {
      _tag: "Busy", kicker: "3 launches are starting", actions,
    } })
    const row = (id: string, label: string, enabled = true) => ({ id, label, enabled })
    const { rerender } = render(<PicoSurface host={host} model={busy([
      row("a", "Cancel Tetris"), row("b", "Cancel Spelunky"), row("c", "Cancel Celeste"),
    ])} />)
    fireEvent.focus(screen.getByRole("button", { name: "Cancel Spelunky" }))
    screen.getByRole("button", { name: "Cancel Spelunky" }).focus()
    rerender(<PicoSurface host={host} model={busy([
      row("a", "Cancel Tetris"), row("b", "Cancel Spelunky", false), row("c", "Cancel Celeste"),
    ])} />)
    expect(document.activeElement === screen.getByRole("button", { name: "Cancel Celeste" })).toBe(true)
  })

  test("does not repeat cancellation when the host disables its action", () => {
    const host = createFixtureHost()
    render(
      <PicoSurface host={host} model={model({ status: {
        _tag: "Busy", kicker: "Cancelling Wario Land 4",
        actions: [{ id: "cancel-launch", label: "Cancel", enabled: false }],
      } })} />,
    )
    const cancel = screen.getByRole("button", { name: "Cancel" }) as HTMLButtonElement
    expect(cancel.disabled).toBe(true)
    fireEvent.click(cancel)
    expect(host.calls).toEqual([])
  })

  test("says what is happening instead of showing the shelf", () => {
    render(
      <PicoSurface
        host={createFixtureHost()}
        model={model({
          status: { _tag: "Busy", kicker: "STARTING", detail: "Mounting the card" },
        })}
      />,
    )

    expect(screen.getByText("STARTING")).toBeTruthy()
    expect(screen.getByText("Mounting the card")).toBeTruthy()
    expect(screen.queryByRole("button", { name: /Celeste Classic/ })).toBeNull()
  })

  test("names the game that is running", () => {
    render(
      <PicoSurface
        host={createFixtureHost()}
        model={model({
          status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" },
        })}
      />,
    )

    expect(screen.getByText("PLAYING")).toBeTruthy()
    expect(screen.getByText("Hollow Knight")).toBeTruthy()
  })

  test("states a failure against the game it belongs to", () => {
    const host = createFixtureHost()
    render(
      <PicoSurface
        host={host}
        model={model({
          status: {
            _tag: "Problem",
            kicker: "COULD NOT START",
            reason: "zao did not answer.",
            canRetry: true,
            gameTitle: "Tetris",
          },
        })}
      />,
    )

    expect(screen.getByRole("heading", { name: "Tetris" })).toBeTruthy()
    expect(screen.getByText("zao did not answer.")).toBeTruthy()

    fireEvent.click(screen.getByRole("button", { name: "TRY AGAIN" }))
    expect(host.calls).toEqual(["retry"])

    fireEvent.click(screen.getByRole("button", { name: "OK" }))
    expect(host.calls).toEqual(["retry", "dismiss"])
  })

  test("offers no retry when Korri says it cannot be retried", () => {
    render(
      <PicoSurface
        host={createFixtureHost()}
        model={model({
          status: {
            _tag: "Problem",
            kicker: "NOT PLAYABLE",
            reason: "That copy is missing.",
            canRetry: false,
          },
        })}
      />,
    )

    expect(screen.queryByRole("button", { name: "TRY AGAIN" })).toBeNull()
    expect(screen.getByRole("button", { name: "OK" })).toBeTruthy()
  })
})

describe("the Back button", () => {
  test("withdraws a launch-location question before leaving the game", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model()} />)

    fireEvent.click(screen.getByRole("button", { name: /Tetris/ }))
    fireEvent.click(screen.getByRole("button", { name: /PLAY/ }))
    expect(screen.getByText("PLAY WHERE?")).toBeTruthy()

    act(() => host.press("back"))

    // The question is gone; the game's screen is still up.
    expect(screen.queryByText("PLAY WHERE?")).toBeNull()
    expect(screen.getByText("LIBRARY ›")).toBeTruthy()
    expect(host.calls).toEqual([])
  })

  test("acknowledges a failure the user has seen", () => {
    const host = createFixtureHost()
    render(
      <PicoSurface
        host={host}
        model={model({
          status: {
            _tag: "Problem",
            kicker: "COULD NOT START",
            reason: "zao did not answer.",
            canRetry: true,
          },
        })}
      />,
    )

    act(() => host.press("back"))

    expect(host.calls).toEqual(["dismiss"])
  })

  test("falls through to the host when there is nothing local to withdraw", () => {
    // Leaving the surface is the host's decision, not Pico's.
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model()} />)

    act(() => host.press("back"))

    expect(host.calls).toEqual([])
  })
})

describe("presentations Pico does not implement", () => {
  test("serves the gameplay overlay rather than falling back to another surface", () => {
    render(
      <PicoSurface
        host={createFixtureHost()}
        model={model({
          presentation: {
            kind: "gameplay-overlay",
            title: "Hollow Knight",
            controls: [{ id: "resume", label: "Continue playing", enabled: true, destructive: false, dismissOnSuccess: true, interaction: { kind: "command" } }],
            groups: [],
          },
        })}
      />,
    )

    expect(screen.getByRole("dialog", { name: /Hollow Knight/ })).toBeTruthy()
    expect(screen.queryByText("LIBRARY")).toBeNull()
  })
})

describe("a game's own actions", () => {
  const open = (name: RegExp) => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model()} />)
    fireEvent.click(screen.getByRole("button", { name }))
    return host
  }

  test("offers the actions Korri publishes for that game", () => {
    open(/Hollow Knight/)
    expect(screen.getByRole("button", { name: /Remove from device/ })).toBeTruthy()
  })

  test("runs one against the game it belongs to", () => {
    const host = open(/Hollow Knight/)
    fireEvent.click(screen.getByRole("button", { name: /Verify files/ }))
    expect(host.calls).toEqual(["gameAction:hollow:verify"])
  })

  test("asks before a destructive one", () => {
    const host = open(/Hollow Knight/)
    fireEvent.click(screen.getByRole("button", { name: /Remove from device/ }))
    expect(host.calls).toEqual([])
    expect(screen.getByText("REMOVE FROM DEVICE?")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "REMOVE FROM DEVICE" }))
    expect(host.calls).toEqual(["gameAction:hollow:remove"])
  })

  test("shows a disabled action dimmed with Korri's reason", () => {
    open(/Hollow Knight/)
    const button = screen.getByRole("button", { name: /Move to SD/ })
    expect(button.hasAttribute("disabled")).toBe(true)
    expect(screen.getByText("No card inserted")).toBeTruthy()
  })

  test("says nothing at all when Korri offers none", () => {
    open(/Celeste Classic/)
    expect(screen.queryByRole("button", { name: /Verify files/ })).toBeNull()
    expect(screen.queryByText("ACTIONS")).toBeNull()
  })
})
