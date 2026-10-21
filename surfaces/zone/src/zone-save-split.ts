/**
 * A save split (ADR 0014, `SurfaceStatus` ChooseSave) on Zone.
 *
 * Zone asks it the way it asks Korri's Confirm (C31): the same dialog, the
 * kicker as headline, the detail below, one button per answer. ZoneSurface
 * reads the model through `zoneModelOf`, so the status layer, its focus and
 * its Back rule see a Confirm and need nothing new. It lives outside
 * src/state because it formats with Date, which src/state may not use.
 */
import type { SurfaceAction, SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
const two = (n: number) => String(n).padStart(2, "0")

/** "21 Oct 2026 09:18": Zone's day, as GAMES prints it, with the time.
 * When a side's session ended, by that device's clock; display only. */
export function zoneSaveTime(at: number): string {
  const date = new Date(at)
  if (Number.isNaN(date.getTime())) return "--"
  return `${date.getDate()} ${MONTHS[date.getMonth()] ?? ""} ${date.getFullYear()} ${two(date.getHours())}:${two(date.getMinutes())}`
}

type ZoneChooseSave = Extract<SurfaceStatus, { _tag: "ChooseSave" }>

/** A save split's answers: one per side (device and end time, running the
 * side's own action id), then Korri's other actions (Not now). */
export function zoneChooseSaveActions(status: ZoneChooseSave): readonly SurfaceAction[] {
  return [
    ...status.sides.map(side => ({
      id: side.actionId,
      label: `${side.device}, ${zoneSaveTime(side.endedAt)}`,
      enabled: true,
    })),
    ...status.actions,
  ]
}

/** The split as the Confirm Zone already draws. */
export function zoneStatusOf(status: SurfaceStatus): Exclude<SurfaceStatus, ZoneChooseSave> {
  if (status._tag !== "ChooseSave") return status
  return {
    _tag: "Confirm",
    kicker: status.kicker,
    ...(status.detail === undefined ? {} : { detail: status.detail }),
    actions: zoneChooseSaveActions(status),
    gameId: status.gameId,
  }
}

/** The model with a save split read as its Confirm. Same object otherwise. */
export function zoneModelOf(model: SurfaceModel): SurfaceModel {
  return model.status._tag === "ChooseSave" ? { ...model, status: zoneStatusOf(model.status) } : model
}
