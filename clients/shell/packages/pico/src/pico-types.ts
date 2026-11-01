/**
 * Types Pico's state shares with its pages. They lived beside the React
 * components that drew them (PicoHome, PicoStore, PicoStepTrack); the native
 * package draws in main.slint, so they live here.
 */
import type { PicoTextDraft } from "./pico-text-draft"

/** How home lays the library out. */
export type PicoHomeMode = "shelf" | "grid" | "hero"

/** Where the person is in the store. */
export interface PicoStoreShown {
  readonly view: "Providers" | "Results" | "Entry"
  readonly providerId: string | undefined
  readonly typing: PicoTextDraft | undefined
}

/** Where one step of a sequence stands. */
export type PicoStepState = "done" | "skipped" | "now" | "next"

export interface PicoStep {
  readonly label: string
  readonly state: PicoStepState
}
