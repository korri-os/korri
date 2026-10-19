/**
 * The package renders end to end with a real SurfaceModel through both
 * doors: the React component and the treaty's mount adapter.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, render, screen } from "@testing-library/react"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { ZoneSurface, zoneSurface } from "../src/index"

afterEach(cleanup)

describe("ZoneSurface", () => {
  test("lists every catalog title over the ground inside the shell", () => {
    render(<ZoneSurface host={createFixtureHost()} model={fixtureModel} />)
    expect(screen.getByText("Spelunky")).toBeTruthy()
    expect(screen.getByText("The Legend of Zelda: Oracle of Seasons")).toBeTruthy()
    expect(screen.getByText("Time 21:47")).toBeTruthy()
    expect(screen.getByText("korri 0.4.1 · 9a1fb22")).toBeTruthy()
    expect(document.querySelector(".zone-ground")).not.toBeNull()
    expect(document.querySelector(".zone-shell")).not.toBeNull()
  })

  test("hides the timer when Korri publishes no clock", () => {
    render(<ZoneSurface host={createFixtureHost()} model={{ ...fixtureModel, clockLabel: undefined }} />)
    expect(screen.queryByText(/Time/)).toBeNull()
  })

  test("listens to the host's buttons through the one door", () => {
    const host = createFixtureHost()
    render(<ZoneSurface host={host} model={fixtureModel} />)
    host.press("system")
    expect(host.calls).toEqual([])
  })
})

describe("zoneSurface.mount", () => {
  test("mounts, updates and unmounts without React in the host", () => {
    const container = document.createElement("div")
    document.body.append(container)
    const instance = zoneSurface.mount(container, fixtureModel, createFixtureHost())
    expect(zoneSurface.id).toBe("zone")
    expect(zoneSurface.title).toBe("Zone")
    instance.update({ ...fixtureModel, catalog: { _tag: "Empty" } })
    instance.unmount()
    container.remove()
  })
})
