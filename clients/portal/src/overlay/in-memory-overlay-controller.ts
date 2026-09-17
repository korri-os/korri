import type { SessionControls } from "@contracts/generated/korrid"
import type {
  SurfaceGameplayControlValue,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { gameplayOverlayPresentationFrom } from "./overlay-model"

/* A plugin declares its in-game controls, and korrid performs them on the
 * device that runs the game. korrid performs none on Linux yet, so no
 * controller reads a live session. This one serves the declared shape so the
 * overlay surface stays buildable until that host exists. */
export interface OverlayController {
  model(): SurfaceModel
  subscribe(listener: () => void): () => void
  refresh(): Promise<void>
  invoke(controlId: string, value?: SurfaceGameplayControlValue): Promise<void>
  dismiss(): void
  destroy(): void
}

export const IN_MEMORY_OVERLAY_LAUNCH_ID =
  "0123456789abcdef0123456789abcdef"

const fixtureControls: SessionControls = {
  launchId: IN_MEMORY_OVERLAY_LAUNCH_ID,
  title: "Browser gameplay fixture",
  groups: [
    {
      id: "fixture-controls",
      label: "Gameplay",
      controls: [
        {
          id: "fixture-command",
          label: "Open menu",
          enabled: true,
          destructive: false,
          dismissOnSuccess: true,
          interaction: { kind: "command" },
        },
        {
          id: "fixture-toggle",
          label: "Fill screen",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: {
            kind: "toggle",
            payload: { value: false, trueLabel: "On", falseLabel: "Off" },
          },
        },
        {
          id: "fixture-choice",
          label: "Mouse mode",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: {
            kind: "choice",
            payload: {
              value: "trackpad",
              options: [
                { value: "trackpad", label: "Trackpad" },
                { value: "direct", label: "Direct" },
              ],
            },
          },
        },
        {
          id: "fixture-range",
          label: "Sharpness",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: {
            kind: "range",
            payload: { value: 50, min: 0, max: 100, step: 5 },
          },
        },
        {
          id: "fixture-disabled",
          label: "Unavailable control",
          enabled: false,
          disabledReason: "No executor is connected in this fixture.",
          destructive: false,
          dismissOnSuccess: false,
          interaction: { kind: "command" },
        },
        {
          id: "fixture-danger",
          label: "Quit fixture",
          enabled: true,
          destructive: true,
          dismissOnSuccess: true,
          interaction: { kind: "command" },
        },
      ],
    },
  ],
}

export function createInMemoryOverlayController(): OverlayController {
  const model: SurfaceModel = {
    presentation: gameplayOverlayPresentationFrom(fixtureControls),
    catalog: { _tag: "Empty" },
    status: { _tag: "Browsing" },
    actions: [],
    settings: [],
    settingsStatus: { _tag: "Idle" },
  }
  return {
    model: () => model,
    subscribe: () => () => {},
    refresh: async () => {},
    invoke: async () => {},
    dismiss: () => {},
    destroy: () => {},
  }
}
