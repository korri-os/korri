import type { SurfaceLaunchSettings, SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import { createLaunchProgram, launchRowId, launchSettingsFixtures as launch, type LaunchScript } from "../fixtures/fixture-launch-settings"
import { launchScopes } from "../plan/launch-settings"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import { plan } from "../plan/zone-select"
import { ZoneGround } from "../ui/organisms/ZoneGround"
import { ZoneLaunchSettings } from "./ZoneLaunchSettings"

export const name = "Launch settings"
export const note = "A game's launch settings: a runner scope strip, a group per layer, each plugin section captioned, each value's source, DEVICE-ONLY"

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
  scope = 0,
}: {
  readonly editor: Open
  readonly status?: SurfaceSettingsStatus
  readonly script?: readonly LaunchScript[]
  readonly share?: number
  /** The runner scope the strip shows, by its place. */
  readonly scope?: number
}) {
  const program = useRef<ReturnType<typeof createLaunchProgram> | null>(null)
  program.current ??= createLaunchProgram(editor, status, script, undefined, scope)
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
        onClear={() => live.dispatch({ _tag: "PressedClear" })}
        onKey={key => live.dispatch({ _tag: "PressedKey", key })}
        onKeyFocus={keyId => live.dispatch({ _tag: "FocusedKey", keyId })}
        onRowFocus={rowId => live.dispatch({ _tag: "FocusedRow", rowId })}
        onRowPress={rowId => live.dispatch({ _tag: "PressedRow", rowId })}
        onRowStep={(rowId, direction) => live.dispatch({ _tag: "SteppedRow", rowId, direction })}
        scope={snapshot.scope}
        status={status ?? { _tag: "Idle" }}
        strips={layout.strips}
        view={snapshot.state}
      />
    </div>
  )
}

const focus = (rowId: string): LaunchScript => ({ _tag: "FocusedRow", rowId })
const scopeOf = (editor: Open, label: string) => Math.max(0, launchScopes(editor).findIndex(scope => scope.label === label))
const MGBA = "game:wario/runner:retroarch/mgba"

/** Any runner, every layer, the Device first: its runner choice, limits and routine. */
export default function Ready() {
  return <Live editor={launch.ready} />
}
/** mGBA: the Device place sets the CPU floor itself, an overridden row. */
export function Overridden() {
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "mGBA")} script={[focus(launchRowId("device/runner:retroarch/mgba", "cpu.min"))]} />
}
/** mGBA's settings on the Game, a captioned run per section: a number, its DEFAULT row, an enum, ADD SETTING. */
export function RunnerSettings() {
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "mGBA")} script={[focus(launchRowId(MGBA, "setting:video.frameskip"))]} />
}
/** Colour correction cleared on the Game: DEFAULT, and mGBA's own default as its source. */
export function ClearedToDefault() {
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "mGBA")} script={[focus(launchRowId(MGBA, "setting:video.color-correction"))]} />
}
/** The Device for mGBA sets an FPS limit korrid does not apply: Korri's note, and the row says why. */
export function SkippedValue() {
  return <Live editor={launch.skipped} scope={scopeOf(launch.skipped, "mGBA")} script={[focus(launchRowId("device/runner:retroarch/mgba", "setting:video.fps-limit"))]} />
}
/** Moonlight's own scope: its settings by section, each row applying when this device streams the game. */
export function Moonlight() {
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "Moonlight")} script={[focus(launchRowId("device/runner:@korri:moonlight/moonlight", "setting:video.codec"))]} />
}
/** Sunshine's sections on the Device: its bitrate cap and encoder, device-only. */
export function Sunshine() {
  return <Live editor={launch.ready} script={[focus(launchRowId("device/any", "service:@korri:sunshine/sunshine:setting:video.max-bitrate-kbps"))]} />
}
/** ADD SETTING with its candidate moved one step: A adds it. */
export function AddSetting() {
  const row = launchRowId(MGBA, "add-setting:video")
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "mGBA")} script={[focus(row), { _tag: "SteppedRow", rowId: row, direction: 1 }]} />
}
/** Frameskip on the keyboard, with INHERIT beside it. */
export function NumberEditor() {
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "mGBA")} script={[{ _tag: "PressedRow", rowId: launchRowId(MGBA, "setting:video.frameskip") }]} />
}
/** gpSP is not the runner in use: its rows say so. */
export function NotInForce() {
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "gpSP")} />
}
/** Values only files set, read-only with their source: the Game's environment and folder. */
export function FileOnly() {
  return <Live editor={launch.ready} script={[focus(launchRowId("game:wario/any", "file:cwd"))]} />
}
/** The Game: inherited rows name their source; the profile pick and New profile. */
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
  const row = launchRowId("game:wario/any", "gpu.max")
  return <Live editor={launch.ready} script={[focus(row)]} status={{ _tag: "Saving", settingId: row }} />
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
/** The Game names a runner that isn't installed: Korri's line, and the row says so. */
export function RunnerUnavailable() {
  return <Live editor={launch.runnerUnavailable} script={[focus(launchRowId("game:wario/any", "runner"))]} />
}
/** gpSP's dry run failed: Korri's line, and a read-only row in gpSP's places. */
export function RunnerFactsUnavailable() {
  return <Live editor={launch.factsUnavailable} scope={scopeOf(launch.factsUnavailable, "gpSP")} script={[focus(launchRowId("device/runner:retroarch/gpsp", "runner-facts:unavailable"))]} />
}
/** The Device's own hooks: the routine night-mode it uses, then its own step, in force. */
export function HooksFromPlacesAndRoutine() {
  return <Live editor={launch.ready} script={[focus(launchRowId("device/any", "file:hooks:before:0"))]} />
}
/** The Game's own hook for the RetroArch family, in force while mGBA runs. */
export function HooksOnGame() {
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "Family retroarch")} script={[focus(launchRowId("game:wario/family:retroarch", "file:hooks:before:0"))]} />
}
/** gpSP's own hook on the Game: not in force while mGBA runs. */
export function HooksNotInForce() {
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "gpSP")} script={[focus(launchRowId("game:wario/runner:retroarch/gpsp", "file:hooks:before:own"))]} />
}
/** mGBA's Language on the Device: a string, its INHERIT LANGUAGE row under it. */
export function StringSetting() {
  return <Live editor={launch.ready} scope={scopeOf(launch.ready, "mGBA")} script={[focus(launchRowId("device/runner:retroarch/mgba", "setting:locale.language"))]} />
}
/** Battery saver not picked: Gamescope inherits On from the Device, and says so. */
export function GamescopeInherited() {
  return <Live editor={launch.noProfilePicked} script={[focus(launchRowId("game:wario/any", "modifier:korri/gamescope:enabled"))]} />
}
/** Battery saver turns Gamescope off for itself. */
export function GamescopeOffInProfile() {
  return <Live editor={launch.ready} script={[focus(launchRowId("profile:battery/any", "modifier:korri/gamescope:enabled"))]} />
}
/** MangoHud describes no settings: on, off or Inherit, and nothing to add. */
export function ModifierNoSchema() {
  return <Live editor={launch.ready} script={[focus(launchRowId("game:wario/any", "modifier:korri/mangohud:enabled"))]} />
}
/** Facts present, but no devfreq GPU: one read-only GPU line. */
export function NoGpu() {
  return <Live editor={launch.noGpu} script={[focus(launchRowId("game:wario/any", "cpu.min"))]} />
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
/** The plugins' own long titles ("Sunshine streaming host · Video") in a narrow column: the captions wrap. */
export function LongCaptions() {
  const long = (title: string) => title.replace(/^Sunshine ·/, "Sunshine streaming host ·").replace(/^Moonlight ·/, "Moonlight stream viewer ·")
  const editor = { ...launch.ready, places: launch.ready.places.map(place => ({ ...place, groups: place.groups.map(group => ({ ...group, title: long(group.title) })) })) }
  return (
    <ZonePartFrame shape="narrow-tall">
      <Live editor={editor} scope={scopeOf(editor, "Moonlight")} share={0.36} script={[focus(launchRowId("device/runner:@korri:moonlight/moonlight", "setting:video.fps"))]} />
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
