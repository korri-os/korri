import "./ZoneSurface.css"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { useCallback, useEffect, useRef } from "react"
import { performZoneRequest, subscribeZoneHostButtons } from "./zone-host"
import { useZoneIdle } from "./use-zone-idle"
import { useZonePlan } from "./use-zone-plan"
import { useZoneProgram } from "./use-zone-program"
import { update, ZONE_START } from "./state/navigation"
import { ZoneGround } from "./ui/organisms/ZoneGround"
import { ZoneShell } from "./ui/templates/ZoneShell"

/**
 * The composition root: the only file that reads the treaty's model and
 * host. It runs the navigation program, opens the one door to Korri, and
 * mounts the shell over the ground.
 *
 * The body lists the catalog's titles as plain text for now; the card band,
 * the tabs and the foot's own shapes arrive with later slices.
 */
export function ZoneSurface({ model, host }: { readonly model: SurfaceModel; readonly host: SurfaceHost }) {
  const root = useRef<HTMLDivElement | null>(null)
  const perform = useCallback((request: Parameters<typeof performZoneRequest>[1]) =>
    performZoneRequest(host, request), [host])
  const program = useZoneProgram(() => ZONE_START, update, model, perform)
  const { dispatch } = program

  useEffect(() => subscribeZoneHostButtons(host, dispatch), [host, dispatch])
  useEffect(() => {
    dispatch({ _tag: "KorriPublished" })
  }, [model, dispatch])
  useZoneIdle(root, program.model, dispatch)
  const layout = useZonePlan(root)

  const titles = model.catalog._tag === "Ready" ? model.catalog.games : []
  return (
    <div className="zone-theme zone-screen" ref={root}>
      <ZoneGround />
      <ZoneShell
        band={layout.band}
        foot={model.clockLabel === undefined ? undefined : <span>Time {model.clockLabel}</span>}
        footEnd={model.buildLabel === undefined ? undefined : <span>{model.buildLabel}</span>}
        hintBack={<span>B Back</span>}
        hintConfirm={<span>A Confirm</span>}
        strips={layout.strips}
        tabs={
          <>
            <span>Games</span>
            <span>Options</span>
            <span>Save data</span>
          </>
        }
      >
        <ul className="zone-surface-titles" data-layer={program.model.layers.at(-1) ?? "none"}>
          {titles.map(game => (
            <li className="zone-surface-title" key={game.id}>{game.title}</li>
          ))}
        </ul>
      </ZoneShell>
    </div>
  )
}
