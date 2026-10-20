/**
 * The RUN WITH page as one value, and the one function that changes it.
 *
 *     focused   the row or action the cursor is on, as
 *               src/plan/options-run-with.ts names it
 *
 * Korri owns the runner choice: its state, routes, saved choices and every
 * action, each bound by the host to the read it came from. Zone only keeps
 * the cursor and hands an action id back unchanged.
 *
 * Back does nothing here. The host publishes the way out (its own cancel
 * action) as an action; Zone never infers one from a label, so it never
 * guesses which action Back would mean.
 */
import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import { runWithPlan } from "../plan/options-run-with"
import { ask, stay, type ZoneStep } from "./requests"

export interface ZoneRunWith {
  readonly focused: string | undefined
}

export type ZoneRunWithMessage =
  | { readonly _tag: "KorriPublished" }
  | { readonly _tag: "PressedBack" }
  | { readonly _tag: "FocusedRow"; readonly rowId: string }
  /** A on an action: a route's or the content's. */
  | { readonly _tag: "PressedAction"; readonly actionId: string }

export const RUN_WITH_START: ZoneRunWith = { focused: undefined }

export function update(
  state: ZoneRunWith,
  message: ZoneRunWithMessage,
  korri: { readonly runnerChoice?: SurfaceRunnerChoice },
): ZoneStep<ZoneRunWith> {
  const choice = korri.runnerChoice
  if (choice === undefined || choice._tag === "Closed") {
    return state.focused === undefined ? stay(state) : stay(RUN_WITH_START)
  }
  const plan = runWithPlan(choice)
  switch (message._tag) {
    case "KorriPublished":
      // The cursor leaves a row Korri took away; the page puts it on the first.
      return state.focused === undefined || plan.ids.includes(state.focused) ? stay(state) : stay(RUN_WITH_START)
    case "PressedBack":
      return stay(state)
    case "FocusedRow":
      return plan.ids.includes(message.rowId) ? stay({ focused: message.rowId }) : stay(state)
    case "PressedAction": {
      const action = plan.actions.get(message.actionId)
      return action === undefined || !action.enabled ? stay(state)
        : ask(state, { _tag: "RunAction", actionId: action.id })
    }
  }
}
