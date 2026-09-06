import type { PicoInitialView } from "../src/pico-initial-view"
import { PicoHome } from "../src/pages/PicoHome"
import { PicoLibrary } from "../src/pages/PicoLibrary"
import { PicoSettings } from "../src/pages/PicoSettings"
import { PicoGameDetail } from "../src/pages/PicoGameDetail"
import { PicoOverlay } from "../src/pages/PicoOverlay"
import { authoredPicoRoot, type AuthoredStory } from "./render-part"

export interface PicoFixtureSeed {
  readonly source: string
  readonly entry?: PicoInitialView
}
export type PicoPageStory = AuthoredStory & { readonly layer?: string }

/** Translate the authored page root into initial navigation, not a second list
 * of parts. Discovery stays automatic; new page roots must opt into real wiring
 * here instead of silently pretending that a static wrapper is interactive. */
export function pageSeed(story: PicoPageStory, binding: {
  readonly sourceId: string
  readonly inputValues: Readonly<Record<string, unknown>>
}): PicoFixtureSeed | null {
  if (story.layer !== "page") return null
  const root = authoredPicoRoot(story)
  const values = { ...root.props, ...binding.inputValues }
  const source = binding.sourceId
  // Caliper HMR revises the component module URL. Compare its stable exported
  // function name as well as identity; this adapter runs only in Vite dev, not
  // against minified names or unrelated projects' part trees.
  const matches = (page: { readonly name: string }) => root.type === page ||
    (typeof root.type === "function" && root.type.name === page.name)
  if (matches(PicoHome)) return { source, entry: { _tag: "Home",
    mode: values.mode === "grid" || values.mode === "hero" ? values.mode : "shelf" } }
  if (matches(PicoLibrary)) return { source, entry: { _tag: "Find",
    order: values.order === "title" || values.order === "played" || values.order === "recent" ? values.order : "korri",
    section: typeof values.section === "string" ? values.section : undefined } }
  if (matches(PicoSettings)) return { source, entry: { _tag: "Settings" } }
  if (matches(PicoOverlay)) return { source, entry: { _tag: "Overlay" } }
  if (matches(PicoGameDetail)) {
    const game = root.props.game
    if (typeof game === "object" && game !== null && "id" in game && typeof game.id === "string") {
      return { source, entry: { _tag: "Detail", gameId: game.id } }
    }
  }
  throw new Error("This Pico page has no live fixture navigation binding")
}

/** Caliper carries an opaque seed; only this adapter creates object seeds. */
export function readFixtureSeed(value: unknown): PicoFixtureSeed {
  if (typeof value === "string") return { source: value }
  if (typeof value === "object" && value !== null && "source" in value && typeof value.source === "string") {
    return value as PicoFixtureSeed
  }
  return { source: "ready" }
}
