/**
 * The frame every screen shares (PicoScreenShell, PicoPanelScreen): where you
 * are in the header, the clock, the palette, and the floor row with the
 * lead control, the readout and the hints.
 */
import type { ButtonView, HintView } from "./kit"
import { button } from "./kit"

export interface StepView { readonly state: string }

export interface FrameView {
  /** The first word of the header, in white: "korri", "LIBRARY", a game. */
  readonly place: string
  /** A breadcrumb mark between place and label. */
  readonly sep: boolean
  /** Where you are, in the accent. */
  readonly label: string
  readonly clock: string
  readonly readout: string
  readonly hints: readonly HintView[]
  readonly hasLead: boolean
  /** "menu" is home's MENU key; "plain" a quiet button (RELOAD). */
  readonly leadKind: string
  readonly lead: ButtonView
  /** Setup's step track; empty elsewhere. */
  readonly track: readonly StepView[]
}

export function frame(options: Partial<FrameView> & { readonly label: string }): FrameView {
  return {
    place: "korri",
    sep: false,
    clock: "",
    readout: "",
    hints: [],
    hasLead: false,
    leadKind: "menu",
    lead: button(""),
    track: [],
    ...options,
  }
}
