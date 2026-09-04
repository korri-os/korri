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
import type { SurfaceModel } from "@contracts/surface/korri-surface"
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

    // No fixture game has art, so every cart shows initials from its title.
    expect(screen.getByText("CC")).toBeTruthy()
    expect(screen.getByText("HK")).toBeTruthy()
  })

  test("follows focus with the caption and the position", () => {
    render(<PicoSurface host={createFixtureHost()} model={model()} />)

    fireEvent.focus(screen.getByRole("button", { name: /Tetris/ }))

    expect(screen.getByRole("heading", { name: "Tetris" })).toBeTruthy()
    expect(screen.getByLabelText("3 of 4")).toBeTruthy()
  })

  test("shows the clock Korri preformatted", () => {
    render(<PicoSurface host={createFixtureHost()} model={model()} />)

    expect(screen.getByText("10:24")).toBeTruthy()
  })
})

describe("launching", () => {
  test("launches directly when Korri offers no choice", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model()} />)

    fireEvent.click(screen.getByRole("button", { name: /Celeste Classic/ }))

    expect(host.calls).toEqual(["launch:celeste"])
  })

  test("asks where to play rather than picking for the user", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model()} />)

    fireEvent.click(screen.getByRole("button", { name: /Tetris/ }))

    // Nothing has been launched yet: the question is the whole point.
    expect(host.calls).toEqual([])
    expect(screen.getByText("PLAY WHERE?")).toBeTruthy()
    expect(screen.getByRole("button", { name: "This device" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "zao" })).toBeTruthy()
  })

  test("sends the location the user chose, unchanged", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model()} />)

    fireEvent.click(screen.getByRole("button", { name: /Tetris/ }))
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

    expect(screen.getByText("Tetris — zao did not answer.")).toBeTruthy()

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
  test("withdraws a launch-location question without launching", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model()} />)

    fireEvent.click(screen.getByRole("button", { name: /Tetris/ }))
    expect(screen.getByText("PLAY WHERE?")).toBeTruthy()

    act(() => host.press("back"))

    expect(screen.queryByText("PLAY WHERE?")).toBeNull()
    expect(screen.getByRole("button", { name: /Tetris/ })).toBeTruthy()
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
  test("draws nothing over a running game rather than the library", () => {
    const { container } = render(
      <PicoSurface
        host={createFixtureHost()}
        model={model({
          presentation: {
            kind: "gameplay-overlay",
            controls: [],
            groups: [],
          },
        })}
      />,
    )

    expect(container.innerHTML).toBe("")
  })
})
