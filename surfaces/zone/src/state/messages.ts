/**
 * Every fact that can change Zone's navigation.
 *
 * Each is named for what happened, never for what should follow. A shape
 * reports a fact through a callback; `update` alone decides what it means.
 * Direction and confirmation never arrive as messages: the host moves DOM
 * focus and clicks, and the shapes report `Focused…` and `Chose…`.
 */
import type { ZoneTab } from "./navigation"

/** The host's buttons. They arrive through the host, not the DOM. */
export type ZoneHostButton =
  | { readonly _tag: "PressedBack" }
  | { readonly _tag: "PressedOptions" }
  | { readonly _tag: "PressedMenu" }
  | { readonly _tag: "PressedSystem" }

export type ZoneMessage =
  | ZoneHostButton
  /** Korri published a new model. The layers it owns are settled against it. */
  | { readonly _tag: "KorriPublished" }
  /** Nothing was pressed for ZONE_IDLE_AFTER_MS. */
  | { readonly _tag: "IdleElapsed" }
  /** A key, a pointer or a focus move on the screen while idle showed. */
  | { readonly _tag: "Woke" }
  // The tabs
  /** The cursor is on a tab. The page below does not change. */
  | { readonly _tag: "FocusedTab"; readonly tab: ZoneTab }
  /** A on a tab: the page changes. */
  | { readonly _tag: "ChoseTab"; readonly tab: ZoneTab }
  // The card band
  | { readonly _tag: "FocusedCard"; readonly index: number }
  /** A on a card: the cursor drops into its action row. */
  | { readonly _tag: "ChoseCard"; readonly index: number }
  // The action row
  | { readonly _tag: "FocusedAction"; readonly index: number }
  /** A on PLAY, CONTINUE or PLAY ON a device. */
  | { readonly _tag: "ChoseLaunch"; readonly gameId: string; readonly locationId?: string }

export const isHostButton = (message: ZoneMessage): message is ZoneHostButton =>
  message._tag === "PressedBack" || message._tag === "PressedOptions"
  || message._tag === "PressedMenu" || message._tag === "PressedSystem"
