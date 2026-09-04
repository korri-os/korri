/**
 * Pico end to end, through the treaty and nothing else.
 *
 * Every assertion is about content the user would see or a command Korri would
 * receive. A test that only proved the surface mounted would pass against a
 * blank screen, which is the exact failure a fixture-fed surface produces when
 * its data never arrives.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
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
