/**
 * Fixture data for the dialog and entry vocabulary, typed against the
 * treaty so a part can only show what Korri can publish: settings rows for
 * the OPTIONS groups, host actions for the busy strip and the dialog, and
 * the kickers and reasons Korri would state verbatim.
 */
import type {
  SurfaceAction,
  SurfaceSettingGroup,
  SurfaceSettingItem,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"

export const fixtureChoiceRow: SurfaceSettingItem = {
  id: "screen-filter",
  label: "Screen filter",
  value: "None",
  description: "How the picture is scaled to the screen",
  interaction: {
    kind: "choice",
    choices: [
      { value: "none", label: "None" },
      { value: "sharp", label: "Sharp" },
      { value: "smooth", label: "Smooth" },
    ],
  },
}

export const fixtureTextRow: SurfaceSettingItem = {
  id: "device-name",
  label: "Name",
  value: "mini",
  description: "What other devices call this one",
  interaction: { kind: "text", placeholder: "Name", maxLength: 24 },
}

export const fixtureSecretRow: SurfaceSettingItem = {
  id: "wifi-password",
  label: "Wi-Fi password",
  value: "hummingbird",
  interaction: { kind: "sensitiveText", maxLength: 63, clearLabel: "Clear" },
}

export const fixtureActionRow: SurfaceSettingItem = {
  id: "scan",
  label: "Wi-Fi",
  value: "Scan",
  description: "Look for networks in range",
  interaction: { kind: "action", actionId: "scan" },
}

export const fixtureDestructiveRow: SurfaceSettingItem = {
  id: "reset",
  label: "Saved networks",
  value: "Forget all",
  interaction: {
    kind: "action",
    actionId: "forget-all",
    destructive: true,
    confirmation: { title: "Forget all networks?", message: "Every saved password is removed.", confirmLabel: "Forget" },
  },
}

export const fixtureReadOnlyRow: SurfaceSettingItem = {
  id: "build",
  label: "Build",
  value: "korri 0.4.1",
}

export const fixtureLongRow: SurfaceSettingItem = {
  id: "controller-mapping-profile",
  label: "Controller mapping profile for the second player",
  value: "Supercalifragilisticexpialidocious-controller-profile",
  interaction: { kind: "choice", choices: [{ value: "a", label: "A" }] },
}

/** Twelve rows, as the VIDEO and WI-FI groups would list together. */
export const fixtureManyRows: readonly SurfaceSettingItem[] = [
  fixtureChoiceRow,
  { id: "window-size", label: "Window size", value: "3x", interaction: { kind: "choice", choices: [] } },
  { id: "window-border", label: "Window border", value: "On", interaction: { kind: "choice", choices: [] } },
  { id: "full-screen", label: "Full screen", value: "Off", interaction: { kind: "choice", choices: [] } },
  { id: "vsync", label: "Vsync", value: "On", interaction: { kind: "choice", choices: [] } },
  { id: "triple-buffering", label: "Triple buffering", value: "Off", interaction: { kind: "choice", choices: [] } },
  fixtureTextRow,
  fixtureSecretRow,
  fixtureActionRow,
  fixtureDestructiveRow,
  fixtureReadOnlyRow,
  { id: "sound", label: "Sound", value: "On", interaction: { kind: "choice", choices: [] } },
]

export const fixtureVideoGroup: SurfaceSettingGroup = {
  title: "Video",
  items: fixtureManyRows.slice(0, 5),
}

export const fixtureCancel: SurfaceAction = { id: "cancel", label: "Cancel", enabled: true }
export const fixtureStop: SurfaceAction = { id: "stop", label: "Stop", enabled: true, destructive: true }

export const fixtureBusy: SurfaceStatus = {
  _tag: "Busy",
  kicker: "Starting",
  detail: "Waiting for zao to answer",
  actions: [fixtureCancel],
  gameId: "spelunky",
}

export const fixtureProblem: SurfaceStatus = {
  _tag: "Problem",
  kicker: "Could not start",
  reason: "zao did not answer in time. Check that it is awake and on the same network.",
  canRetry: true,
  gameId: "spelunky",
  gameTitle: "Spelunky",
}

/** A 40-character draft, the longest the keyboard takes. Plain words, so
 * the secret scanner does not mistake a fixture for a key. */
export const fixtureDraft40 = "quick-brown-fox-jumps-over-the-lazy-dog!"

/** A 200-character string, the largest the QR well encodes. */
export const fixtureBackup200 = "quick-brown-fox-jumps-over-the-lazy-dog-".repeat(5)
