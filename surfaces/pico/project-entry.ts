/**
 * Pico's Caliper profile: what a design tool mounts, and at what real size.
 *
 * In the checkout on purpose. This lived in an external profile directory for
 * the first slice, and that directory pointed at a launcher checkout that later
 * disappeared — taking the whole review environment with it. A profile that
 * only exists on one machine is not a review environment, it is a bookmark.
 * Keeping it here also means it is reviewed when the contract it targets moves,
 * which is exactly what went wrong: the old file exported `surfaces`, Caliper
 * now reads `adapters`, and the launcher threw on every start.
 *
 * Nothing here is imported by the surface: the treaty still allows Pico four
 * imports, and this file is on the outside of that boundary looking in.
 */
import { picoSurface } from "./src/index"
import { createFixtureHost, fixtureModel } from "./src/preview"

/* Screen sizes in real millimetres, carried over from legacy's calibrated
 * device-lab seeds rather than guessed: these are the panels, not the shells. */
const DEVICES = [
  { id: "rg353m", name: "RG353M", widthMm: 72, heightMm: 52 },
  { id: "thor", name: "THOR", widthMm: 132, heightMm: 76 },
  { id: "odin2portal", name: "ODIN 2 PORTAL", widthMm: 156, heightMm: 85 },
]

/**
 * One adapter: Pico has one surface, and it mounts through the same treaty
 * entry point Korri uses. The seed is the fixture model the tests run against,
 * so what a reviewer sees and what the suite asserts cannot drift apart.
 */
export const adapters = [
  {
    id: picoSurface.id,
    devices: DEVICES,
    makeSeedInitialValues: async () => undefined,
    mountSurface(host: HTMLElement) {
      const mounted = picoSurface.mount(host, fixtureModel, createFixtureHost())
      /* `router: null` and `dispose`, not `unmount`: Pico has no router of its
       * own — a surface receives one model and renders it — and the lab tears a
       * mount down through `dispose`.
       *
       * Deferred by a microtask because the lab disposes while React is still
       * rendering, and unmounting a root mid-render races. Korri calls
       * `unmount` from outside a render, so this is the tool's timing absorbed
       * at the edge rather than leaked into the surface. */
      return { router: null, dispose: () => queueMicrotask(() => mounted.unmount()) }
    },
  },
]

export const initialLabPath = "/lab/all/pico/"
