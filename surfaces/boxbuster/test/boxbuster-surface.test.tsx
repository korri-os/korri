/**
 * The door: what Boxbuster shows while there is no store to walk into.
 *
 * The open store is a WebGL canvas, which happy-dom cannot draw; its contract
 * is the store view, tested in boxbuster-store-view.test.ts.
 */
import { afterEach, describe, expect, test } from "bun:test"
import type {
  SurfaceCatalog,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { BoxbusterSurface } from "../src/BoxbusterSurface"
import { createRecordingHost } from "./recording-host"

afterEach(cleanup)

function modelWith(catalog: SurfaceCatalog): SurfaceModel {
  return {
    presentation: { kind: "catalog" },
    catalog,
    status: { _tag: "Browsing" },
    actions: [],
    settings: [],
    settingsStatus: { _tag: "Idle" },
  }
}

describe("the Boxbuster door", () => {
  test("says the store is opening while the catalog loads", () => {
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith({ _tag: "Loading" })}
      />,
    )
    expect(screen.getByRole("heading").textContent).toBe("Opening the store…")
  })

  test("says the shelves are empty for an empty library", () => {
    render(
      <BoxbusterSurface
        host={createRecordingHost()}
        model={modelWith({ _tag: "Empty" })}
      />,
    )
    expect(screen.getByRole("heading").textContent).toBe(
      "The shelves are empty",
    )
  })

  test("states Korri's error and retries with one confirm", () => {
    const host = createRecordingHost()
    render(
      <BoxbusterSurface
        host={host}
        model={modelWith({ _tag: "Error", message: "korrid did not answer" })}
      />,
    )
    expect(screen.getByText("korrid did not answer")).toBeTruthy()
    const retry = screen.getByRole("button", { name: "Try again" })
    // Focus is where the host's confirm lands, so it must start on the action.
    expect(document.activeElement).toBe(retry)
    fireEvent.click(retry)
    expect(host.calls).toEqual(["reload"])
  })
})
