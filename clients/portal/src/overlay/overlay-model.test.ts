import { describe, expect, test } from "bun:test"
import type { SessionControls } from "@contracts/generated/korrid"
import { gameplayOverlayPresentationFrom } from "./overlay-model"

const controls: SessionControls = {
  launchId: "launch-1",
  title: "Skate 3",
  groups: [
    {
      id: "@korri:runner",
      label: "Streaming",
      controls: [
        {
          id: "keyboard",
          label: "Keyboard",
          enabled: true,
          destructive: false,
          dismissOnSuccess: true,
          interaction: { kind: "command" },
        },
        {
          id: "fill",
          label: "Fill screen",
          description: "Crop the stream to fill the display.",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: {
            kind: "toggle",
            payload: { value: true, trueLabel: "crop to fill", falseLabel: "fit (letterbox)" },
          },
        },
        {
          id: "mouse-mode",
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
          id: "sharpness",
          label: "Sharpness",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: {
            kind: "range",
            payload: {
              value: 50,
              min: 0,
              max: 100,
              step: 5,
            },
          },
        },
      ],
    },
  ],
}

describe("gameplayOverlayPresentationFrom", () => {
  test("materializes every control form without losing presentation facts", () => {
    const presentation = gameplayOverlayPresentationFrom(controls)

    expect(presentation.kind).toBe("gameplay-overlay")
    expect(presentation.title).toBe("Skate 3")
    expect(presentation.controls.map(control => control.id)).toEqual([
      "overlay:return",
      "overlay:end",
      "overlay:open-korri",
    ])
    expect(presentation.groups.map(group => group.label)).toEqual(["Streaming"])
    expect(
      presentation.groups[0]?.controls.map(control => ({
        id: control.id,
        label: control.label,
        description: control.description,
        enabled: control.enabled,
        disabledReason: control.disabledReason,
        destructive: control.destructive,
        dismissOnSuccess: control.dismissOnSuccess,
        interaction: control.interaction,
      })),
    ).toEqual([
      {
        id: "keyboard",
        label: "Keyboard",
        description: undefined,
        enabled: true,
        disabledReason: undefined,
        destructive: false,
        dismissOnSuccess: true,
        interaction: { kind: "command" },
      },
      {
        id: "fill",
        label: "Fill screen",
        description: "Crop the stream to fill the display.",
        enabled: true,
        disabledReason: undefined,
        destructive: false,
        dismissOnSuccess: false,
        interaction: {
          kind: "toggle",
          value: true,
          trueLabel: "crop to fill",
          falseLabel: "fit (letterbox)",
        },
      },
      {
        id: "mouse-mode",
        label: "Mouse mode",
        description: undefined,
        enabled: true,
        disabledReason: undefined,
        destructive: false,
        dismissOnSuccess: false,
        interaction: {
          kind: "choice",
          value: "trackpad",
          options: [
            { value: "trackpad", label: "Trackpad" },
            { value: "direct", label: "Direct" },
          ],
        },
      },
      {
        id: "sharpness",
        label: "Sharpness",
        description: undefined,
        enabled: true,
        disabledReason: undefined,
        destructive: false,
        dismissOnSuccess: false,
        interaction: {
          kind: "range",
          value: 50,
          min: 0,
          max: 100,
          step: 5,
        },
      },
    ])
  })

  test("keeps the three core controls available and removes empty or unsupported plugin groups", () => {
    const presentation = gameplayOverlayPresentationFrom({
      launchId: "launch-2",
      groups: [
        { id: "empty", label: "Empty plugin", controls: [] },
        {
          id: "unsupported",
          label: "Unsupported plugin",
          controls: [{
            id: "not-live",
            label: "Not live",
            enabled: false,
            disabledReason: "No executor is available.",
            destructive: false,
            dismissOnSuccess: false,
            interaction: { kind: "command" },
          }],
        },
      ],
    })

    expect(presentation).toEqual({
      kind: "gameplay-overlay",
      controls: [
        {
          id: "overlay:return",
          label: "Return",
          enabled: true,
          destructive: false,
          dismissOnSuccess: true,
          interaction: { kind: "command" },
        },
        {
          id: "overlay:end",
          label: "End game",
          description: "Unsaved progress will be lost.",
          enabled: true,
          destructive: true,
          dismissOnSuccess: true,
          interaction: { kind: "command" },
        },
        {
          id: "overlay:open-korri",
          label: "Open Korri",
          enabled: true,
          destructive: false,
          dismissOnSuccess: true,
          interaction: { kind: "command" },
        },
      ],
      groups: [],
    })
  })

  test("publishes no integration effect or protected instruction fields", () => {
    const serialized = JSON.stringify(gameplayOverlayPresentationFrom(controls))

    expect(serialized).not.toContain("effect")
    expect(serialized).not.toContain("instruction")
    expect(serialized).not.toContain("integrity")
    expect(serialized).not.toContain("nonce")
    expect(serialized).not.toContain("launchId")
  })
})
