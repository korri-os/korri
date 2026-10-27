/**
 * The page test/pico-displays-browser.test.ts drives in Chromium: Pico
 * mounted through its treaty entry, as the portal mounts it, in the order the
 * RG DS kiosk saw things before ADR 0039's 2026-10-27 amendment: the window opens on the main display only, the
 * host states both displays, the library arrives, and only then does the
 * window grow to cover both displays.
 */
import "../../src/pico-tokens.css"
import "../../src/pico-fonts.css"
import "../../src/pico.css"
import { rgdsDisplays } from "../../src/fixtures/displays"
import { createFixtureHost, fixtureModel } from "../../src/fixtures/fixture-host"
import { picoSurface } from "../../src/mount"

const loading = { ...fixtureModel, catalog: { _tag: "Loading" } } as const
const root = document.getElementById("root")
if (root === null) throw new Error("no #root")
const instance = picoSurface.mount(root, loading, createFixtureHost())

declare global {
  interface Window {
    picoStep: (step: "displays" | "ready") => void
  }
}

window.picoStep = step => {
  instance.update(step === "displays" ? { ...loading, displays: rgdsDisplays } : { ...fixtureModel, displays: rgdsDisplays })
}
