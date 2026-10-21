import type { SurfaceLaunchSettings, SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import { createLaunchProgram, launchSettingsFixtures as launch, type LaunchScript } from "../fixtures/fixture-launch-settings"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import { plan } from "../plan/zone-select"
import { ZoneGround } from "../ui/organisms/ZoneGround"
import { ZoneLaunchSettings } from "./ZoneLaunchSettings"

export const name = "Launch settings"
export const note = "A game's launch settings: a group per layer, each value's source, DEVICE-ONLY, applies at next launch"

export const composition = {
  default: [
    { part: "src/ui/templates/ZoneShell.template.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneOptionGroup.molecule.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "Source" },
  ],
  Conflict: [{ part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "Remote" }],
  Error: [{ part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "Remote" }],
  Loading: [{ part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "Continue" }],
} as const

type Open = Exclude<SurfaceLaunchSettings, { _tag: "Closed" }>

/** Unexported: the real page on a live program, so focus, steps and the keyboard answer. */
function Live({
  editor,
  status,
  script = [],
  share = 1,
}: {
  readonly editor: Open
  readonly status?: SurfaceSettingsStatus
  readonly script?: readonly LaunchScript[]
  readonly share?: number
}) {
  const program = useRef<ReturnType<typeof createLaunchProgram> | null>(null)
  program.current ??= createLaunchProgram(editor, status, script)
  const live = program.current
  const snapshot = useSyncExternalStore(live.subscribe, live.get)
  const layout = plan(innerWidth * share, innerHeight)
  return (
    <div className="zone-theme zone-screen" data-ground="options">
      <ZoneGround />
      <ZoneLaunchSettings
        band={layout.band}
        build="korri 0.4.1 · 9a1fb22"
        clock="21:47"
        focusedAction={snapshot.action}
        launch={snapshot.launch}
        onActionFocus={live.focusAction}
        onDialogFocus={index => live.dispatch({ _tag: "FocusedDialogButton", index })}
        onDialogPress={index => live.dispatch({ _tag: "PressedDialogButton", index })}
        onKey={key => live.dispatch({ _tag: "PressedKey", key })}
        onKeyFocus={keyId => live.dispatch({ _tag: "FocusedKey", keyId })}
        onRowFocus={rowId => live.dispatch({ _tag: "FocusedRow", rowId })}
        onRowPress={rowId => live.dispatch({ _tag: "PressedRow", rowId })}
        onRowStep={(rowId, direction) => live.dispatch({ _tag: "SteppedRow", rowId, direction })}
        status={status ?? { _tag: "Idle" }}
        strips={layout.strips}
        view={snapshot.state}
      />
    </div>
  )
}

const focus = (rowId: string): LaunchScript => ({ _tag: "FocusedRow", rowId })

/** Every layer, the Device layer first, its CPU floor inherited. */
export default function Ready() {
  return <Live editor={launch.ready} />
}
/** The Runner layer sets the CPU floor itself: an overridden row. */
export function Overridden() {
  return <Live editor={launch.ready} script={[focus("launch:set:runner:retroarch/mgba:cpu.min")]} />
}
/** The Game layer: inherited rows name their source; the profile pick and New profile. */
export function ProfilePicker() {
  return <Live editor={launch.ready} script={[focus("launch:profile:pick")]} />
}
/** The selected profile's own layer: its values, its name and DELETE PROFILE. */
export function ProfileLayer() {
  return <Live editor={launch.ready} script={[focus("launch:profile:rename:battery")]} />
}
export function ProfilesMany() {
  return <Live editor={launch.manyProfiles} script={[focus("launch:profile:rename:word")]} />
}
export function NewProfile() {
  return (
    <Live
      editor={launch.ready}
      script={[{ _tag: "PressedRow", rowId: "launch:profile:new" },
        ...[..."Race"].map(char => ({ _tag: "PressedKey", key: { kind: "char", char } }) as LaunchScript)]}
    />
  )
}
export function ConfirmDeleteProfile() {
  return <Live editor={launch.ready} script={[{ _tag: "PressedRow", rowId: "launch:profile:remove:battery" }]} />
}
export function Saving() {
  return (
    <Live
      editor={launch.ready}
      script={[focus("launch:set:game:gpu.max")]}
      status={{ _tag: "Saving", settingId: "launch:set:game:gpu.max" }}
    />
  )
}
export function Loading() {
  return <Live editor={launch.loading} />
}
export function Conflict() {
  return <Live editor={launch.conflict} />
}
export function Error() {
  return <Live editor={launch.error} />
}
/** No runner chosen: Device, Game and the profile stay; Korri's line and CHOOSE A RUNNER. */
export function NeedsRunnerChoice() {
  return <Live editor={launch.needsRunner} />
}
export function RouteUnavailable() {
  return <Live editor={launch.routeUnavailable} />
}
/** Facts present, but no devfreq GPU: one read-only GPU line. */
export function NoGpu() {
  return <Live editor={launch.noGpu} script={[focus("launch:set:game:cpu.min")]} />
}
/** No device facts: 10 % steps, and the rows say so. */
export function FactsMissing() {
  return <Live editor={launch.factsMissing} />
}
/** A narrow column. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Live editor={launch.ready} share={0.36} />
    </ZonePartFrame>
  )
}
/** A wide, short strip. */
export function WideShort() {
  return (
    <ZonePartFrame shape="wide-short">
      <Live editor={launch.ready} />
    </ZonePartFrame>
  )
}
