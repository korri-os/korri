/**
 * How home presents the library.
 *
 * Korri delivers one catalog; how it is laid out is the surface's decision, and
 * a handheld is used at arm's length in a chair and at a desk with a hundred
 * games. So home has modes, and the treaty's `menu` button cycles them.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const open = () => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={fixtureModel} />)
  return host
}

describe("cycling the mode", () => {
  test("starts on the shelf", () => {
    open()
    expect(screen.getByRole("list", { name: "Shelf" })).toBeTruthy()
  })

  test("menu moves to the grid", () => {
    const host = open()
    act(() => host.press("menu"))
    expect(screen.getByRole("list", { name: "Library by collection" })).toBeTruthy()
    expect(screen.queryByRole("list", { name: "Shelf" })).toBeNull()
  })

  test("menu again reaches the hero, and once more comes home", () => {
    const host = open()
    act(() => host.press("menu"))
    act(() => host.press("menu"))
    expect(screen.getByRole("list", { name: "Resume" })).toBeTruthy()
    act(() => host.press("menu"))
    expect(screen.getByRole("list", { name: "Shelf" })).toBeTruthy()
  })

  test("tells the user which mode they are in", () => {
    const host = open()
    act(() => host.press("menu"))
    expect(screen.getByText("PICO ▸ LIBRARY · GRID")).toBeTruthy()
  })

  test("never asks Korri for anything to change mode", () => {
    const host = open()
    act(() => host.press("menu"))
    act(() => host.press("menu"))
    act(() => host.press("menu"))
    expect(host.calls).toEqual([])
  })
})

describe("the grid", () => {
  const grid = () => {
    const host = open()
    act(() => host.press("menu"))
    return host
  }

  test("groups games under the sections Korri published", () => {
    grid()
    expect(screen.getByRole("heading", { name: "CONTINUE" })).toBeTruthy()
    expect(screen.getByRole("heading", { name: "ZAO" })).toBeTruthy()
    expect(screen.getByRole("heading", { name: "THIS DEVICE" })).toBeTruthy()
  })

  test("keeps every game Korri sent", () => {
    grid()
    for (const title of ["Celeste Classic", "Hollow Knight", "Tetris", "Spelunky"]) {
      expect(screen.getByRole("button", { name: new RegExp(title) })).toBeTruthy()
    }
  })

  test("opens a game the same way the shelf does", () => {
    const host = grid()
    fireEvent.click(screen.getByRole("button", { name: /Spelunky/ }))
    expect(host.calls).toEqual([])
    expect(screen.getByText("PICO ▸ GAME")).toBeTruthy()
  })

  test("holds games Korri did not group under one honest heading", () => {
    const host = createFixtureHost()
    render(
      <PicoSurface
        host={host}
        model={{
          ...fixtureModel,
          catalog: { _tag: "Ready", games: [{ id: "a", title: "Loose Game" }] },
        }}
      />,
    )
    act(() => host.press("menu"))
    // Not "Other" or "Uncategorised" — Korri said nothing, so neither does Pico.
    expect(screen.getByRole("heading", { name: "GAMES" })).toBeTruthy()
  })
})
