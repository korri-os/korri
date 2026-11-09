import type { PicoLayoutPlan, PicoWindow } from "../pico-display-plan"
/**
 * The native package's own state around Pico's programs, and what a screen
 * hands back: a view for main.slint and the focus targets on it.
 */
import type { CatalogQuery, KorriModel, SurfaceIdentityDisposition, SurfaceModel } from "../model"
import type { PicoFontId } from "../pico-fonts"
import type { PicoTextDraft } from "../pico-text-draft"
import type { PicoMessage } from "../state/messages"
import type { PicoNavigation } from "../state/navigation"
import type { OnboardingMessage, OnboardingNav } from "../state/onboarding"
import type { PicoOverlayMessage, PicoOverlayState } from "../state/overlay"
import type { Target } from "./focus"

/** A field of a form that Pico's web build typed into a DOM input. Natively
 * it opens Pico's own keyboard; OK hands the text to the program. */
export interface Typing {
  readonly field: "identity:password" | "identity:secret" | "identity:bunkerUri" | "backup:password"
  readonly label: string
  readonly draft: PicoTextDraft
  readonly secret: boolean
}

export interface State {
  readonly nav: PicoNavigation
  readonly setup: OnboardingNav
  readonly overlay: PicoOverlayState
  readonly font: PicoFontId
  /** The cursor: the id of the focused target, on the screen named `screen`. */
  readonly focus: string | undefined
  readonly screen: string
  /** The cursor over a running game, kept apart: the catalog's survives the pause. */
  readonly overlayFocus: string | undefined
  /** The game Korri last heard of (FocusGame). */
  readonly reported: string | null
  /** The last MoveCatalog sent, so the same one is never sent twice in a row. */
  readonly asked: string | undefined
  /** Sections seen in Korri's windows, in order (Find's collection chips). */
  readonly sections: readonly string[]
  /** Pico's keyboard over a form field. */
  readonly typing: Typing | undefined
  /** Counts the invite notices raised, so main.slint restarts their 8 s. */
  readonly noticeRound: number
  /** The window, from the shell's last `Resized` (contract A2, 8.1). */
  readonly window: PicoWindow
  /** Each region's last focused target, for an exit that enters by `remembered`. */
  readonly remembered: Readonly<Record<string, string>>
  /** A tap came last, not a button (ADR 0055): the focus highlight hides
   * until the next `Pressed`. The cursor stays where it is. */
  readonly touched: boolean
}

export type Dispatch =
  | { readonly to: "nav"; readonly message: PicoMessage }
  | { readonly to: "setup"; readonly message: OnboardingMessage }
  | { readonly to: "overlay"; readonly message: PicoOverlayMessage }
  | { readonly to: "local"; readonly change: (state: State) => State }
  | { readonly to: "focus"; readonly id: string }

export type Act = readonly Dispatch[]

export const nav = (message: PicoMessage): Dispatch => ({ to: "nav", message })
export const setup = (message: OnboardingMessage): Dispatch => ({ to: "setup", message })
export const local = (change: (state: State) => State): Dispatch => ({ to: "local", change })

/** Pico's one layout plan, from the window (src/pico-display-plan.ts). */
export type Plan = PicoLayoutPlan

export interface Ctx {
  readonly s: State
  readonly korri: KorriModel
  readonly model: SurfaceModel
  readonly plan: Plan
}

export interface Screen {
  /** Which screen this is: the cursor resets when it changes. */
  readonly key: string
  readonly view: Readonly<Record<string, unknown>>
  readonly targets: readonly Target<Act>[]
  /** Where the cursor starts, or goes when its target leaves. */
  readonly fallback?: string
  /** Each region's active target (the chosen cart, the open tab), for an
   * exit that enters by `active` (contract 8.4). */
  readonly active?: Readonly<Record<string, string>>
  /** Left and right do this instead of moving, whatever is focused. */
  readonly horizontal?: { readonly left: Act; readonly right: Act }
}

export type { CatalogQuery, SurfaceIdentityDisposition }
