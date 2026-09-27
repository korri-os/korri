import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import { createFixtureHost } from "../fixtures/fixture-host"
import { runnerChoice } from "../fixtures/runner-choice"
import { PicoRunnerPicker } from "./PicoRunnerPicker"

export const name = "Runner Picker"
export const note = "Installed runners on one device; saved missing choices stay visible"

function picker(choice: Exclude<SurfaceRunnerChoice, { _tag: "Closed" }>) {
  return <PicoRunnerPicker choice={choice} onAction={createFixtureHost().runAction} />
}

// Shape and copy follow the host's runner-chooser blank/show/launch presentations.
const cancel = { id: "runner:cancel", label: "Cancel", enabled: true }
const reload = { id: "runner:reload", label: "Reload runners", enabled: true }
function blank(message: string) {
  return { gameTitle: runnerChoice.gameTitle, message, routes: [], saved: [], warnings: [], actions: [cancel] }
}

export function Loading() { return picker({ _tag: "Loading", ...blank("Reading installed runners…") }) }
export function Ready() {
  return picker({ ...runnerChoice, _tag: "Ready", saved: [],
    message: "Choose a runner on this device. Launch once does not change saved choices. A game choice overrides a system choice.",
    actions: [reload, cancel] })
}
export function NoRunners() {
  return picker({ _tag: "Ready", ...blank("Choose a runner on this device. Launch once does not change saved choices. A game choice overrides a system choice."), actions: [reload, cancel] })
}
export function Error() {
  return picker({ _tag: "Error", ...blank("Installed runners could not be read."), actions: [reload, cancel] })
}
export function Conflict() {
  return picker({ _tag: "Conflict", ...blank("Stop Hollow Knight before switching runners. Nothing will launch automatically after stopping."),
    actions: [{ id: "runner:stop", label: "Stop active session", enabled: true }, reload, cancel] })
}
export function Busy() {
  return picker({ _tag: "Busy", ...blank("Launching… Cancel closes this panel; it cannot undo a launch already sent.") })
}
export function Warnings() {
  return picker({ _tag: "Warnings", ...blank("Launched with settings warnings"),
    // The runner-chooser test's warning, in warningText's published format.
    warnings: ["video_driver · retroarch/linux · /nix/store/exact-build: Not supported by pinned version 1.20"],
    actions: [{ ...cancel, label: "Done" }] })
}
// Closed is excluded by PicoRunnerPicker's props: its parent unmounts this page.

export default function PicoRunnerPickerPart() {
  return <PicoRunnerPicker choice={runnerChoice} onAction={() => undefined} />
}
