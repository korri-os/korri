/**
 * Pico's Caliper profile: what a design tool mounts, and at what real size.
 *
 * In the checkout on purpose. This lived in an external profile directory for
 * the first slice, and that directory pointed at a launcher checkout that later
 * disappeared — taking the whole review environment with it. A profile that
 * only exists on one machine is not a review environment, it is a bookmark.
 *
 * Nothing here is imported by the surface: the treaty still allows Pico four
 * imports, and this file is on the outside of that boundary looking in.
 */
import { picoSurface } from "@korri/pico"
import { createFixtureHost, fixtureModel } from "@korri/pico/preview"

/* Screen sizes in real millimetres, carried over from legacy's calibrated
 * device-lab seeds rather than guessed: these are the panels, not the shells. */
export const surfaces = [{
  id: picoSurface.id,
  title: picoSurface.title,
  devices: [
    { id: "rg353m", name: "RG353M", widthMm: 72, heightMm: 52 },
    { id: "thor", name: "THOR", widthMm: 132, heightMm: 76 },
    { id: "odin2portal", name: "ODIN 2 PORTAL", widthMm: 156, heightMm: 85 },
  ],
  mount(host: HTMLElement) {
    const mounted = picoSurface.mount(host, fixtureModel, createFixtureHost())
    return { unmount: () => mounted.unmount() }
  },
}]
export const initialLabPath = "/lab/all/pico/"
