/**
 * Shift Launch settings — one game's launch settings, as Shift reads every
 * setting: the Settings page, its title the game, one group per layer
 * (Device, System, Runner, Game, Release, each Profile), then Korri's own
 * actions for the editor.
 *
 * Every row says where its value in force comes from and that it stays on
 * this device; a row's description is Korri's note ("Applies at next
 * launch."). A percent or a profile pick opens the chooser sheet; a profile's
 * name opens the name sheet; Delete profile asks first, in Korri's words.
 * While Korri reads, or after a change made elsewhere or a failure, the
 * layers give way to Korri's message and its actions. B leaves through
 * Korri's close action: the editor is Korri's to close.
 */
import type {
  SurfaceAction,
  SurfaceLaunchSettings,
  SurfaceSettingGroup,
  SurfaceSettingsStatus,
} from "@contracts/surface/korri-surface"
import { ShiftSettings } from "./ShiftSettings"

/** Korri's editor actions as one group of action rows. */
function actionGroup(actions: readonly SurfaceAction[]): SurfaceSettingGroup[] {
  const enabled = actions.filter(action => action.enabled)
  return enabled.length === 0 ? [] : [{
    title: "Actions",
    items: enabled.map(action => ({
      id: `shift:launch-action:${action.id}`,
      label: action.label,
      ...(action.description === undefined ? {} : { description: action.description }),
      interaction: { kind: "action" as const, actionId: action.id },
    })),
  }]
}

export function ShiftLaunchSettings({
  launch,
  status,
  time,
  onChange,
  onAction,
  onDismissProblem,
}: {
  readonly launch: Exclude<SurfaceLaunchSettings, { _tag: "Closed" }>
  readonly status: SurfaceSettingsStatus
  readonly time?: string
  readonly onChange: (settingId: string, value: string) => void
  readonly onAction: (actionId: string) => void
  readonly onDismissProblem: () => void
}) {
  const close = launch.actions.find(action => action.id.endsWith(":cancel"))
  const layers: SurfaceSettingGroup[] = launch._tag === "Ready"
    ? launch.layers.map(layer => ({ title: layer.title, items: layer.groups.flatMap(group => group.items) }))
    : []
  const notice = launch._tag === "Loading"
    ? "Reading launch settings…"
    : launch._tag === "Conflict" || launch._tag === "Error"
      ? launch.message
      : launch.note
  return (
    <ShiftSettings
      // A new game, or Korri's state changing, starts the cursor on the first row.
      key={`${launch.gameTitle ?? ""}:${launch._tag}`}
      groups={[...layers, ...actionGroup(launch.actions)]}
      kicker="Launch settings"
      title={launch.gameTitle ?? "This game"}
      status={status}
      onChange={onChange}
      onAction={onAction}
      onDismissProblem={onDismissProblem}
      {...(notice === undefined ? {} : { notice })}
      {...(time === undefined ? {} : { time })}
      {...(close === undefined ? {} : { onClose: () => onAction(close.id) })}
    />
  )
}
