import type { PicoHomeMode } from "./pages/PicoHome"
import type { PicoOrder } from "./pico-library-view"

/** Optional starting view for embedded previews. Not a router or model fact:
 * after mounting, ordinary Pico interactions own navigation. Hosts omit it. */
export type PicoInitialView =
  | { readonly _tag: "Home"; readonly mode?: PicoHomeMode }
  | { readonly _tag: "Find"; readonly order?: PicoOrder; readonly section?: string }
  | { readonly _tag: "Settings" }
  | { readonly _tag: "Detail"; readonly gameId: string }
  | { readonly _tag: "Overlay" }
