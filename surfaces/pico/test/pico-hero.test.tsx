/**
 * The hero mode: one game large, with what Korri knows about playing it, and
 * everything resumable beneath.
 *
 * The one presentation decision here is which game leads. Korri publishes no
 * "featured" flag, so Pico picks by a rule it can defend from published data —
 * the game played most recently — and says which rule it used.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const hero = (overrides: Partial<SurfaceModel> = {}) => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={{ ...fixtureModel, ...overrides }} />)
  act(() => host.press("menu"))
  act(() => host.press("menu"))
  return host
}

describe("which game leads", () => {
  test("the one Korri says was played most recently", () => {
    hero()
    // Hollow Knight is the only fixture with a lastPlayedAt.
    expect(screen.getByRole("heading", { name: "Hollow Knight" })).toBeTruthy()
  })

  test("says which rule it used rather than implying an endorsement", () => {
    hero()
    expect(screen.getByText("LAST PLAYED")).toBeTruthy()
  })

  test("falls back to the first game when Korri has timed nothing", () => {
    hero({
      catalog: {
        _tag: "Ready",
        games: [
          { id: "a", title: "Alpha" },
          { id: "b", title: "Beta" },
        ],
      },
    })
    expect(screen.getByRole("heading", { name: "Alpha" })).toBeTruthy()
    expect(screen.queryByText("LAST PLAYED")).toBeNull()
  })
})

describe("what the hero states", () => {
  test("shows the play facts Korri published", () => {
    hero()
    expect(screen.getByText("3")).toBeTruthy()
    expect(screen.getByText("PLAYS")).toBeTruthy()
    expect(screen.getByText("2H 10M")).toBeTruthy()
  })

  test("opens the game rather than launching it", () => {
    const host = hero()
    // The hero itself, not the same game's row in the resume list below it.
    const heroSection = screen.getByRole("region", { name: "Hollow Knight" })
    fireEvent.click(heroSection.querySelector("button")!)
    expect(host.calls).toEqual([])
    expect(screen.getByText("PICO ▸ GAME")).toBeTruthy()
  })
})

describe("what can be resumed", () => {
  test("lists every game Korri marked resumable", () => {
    hero()
    const resume = screen.getByRole("list", { name: "Resume" })
    expect(resume.textContent).toContain("Hollow Knight")
    expect(resume.textContent).not.toContain("Spelunky")
  })

  test("says nothing at all when nothing resumes", () => {
    hero({
      catalog: { _tag: "Ready", games: [{ id: "a", title: "Alpha" }] },
    })
    expect(screen.queryByRole("list", { name: "Resume" })).toBeNull()
  })

  test("opens a resumable game from the list", () => {
    const host = hero()
    const resume = screen.getByRole("list", { name: "Resume" })
    fireEvent.click(resume.querySelector("button")!)
    expect(host.calls).toEqual([])
    expect(screen.getByText("PICO ▸ GAME")).toBeTruthy()
  })
})
