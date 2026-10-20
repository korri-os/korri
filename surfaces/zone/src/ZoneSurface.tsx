import "./ZoneSurface.css"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { useRef } from "react"
import { ZoneGames } from "./pages/ZoneGames"
import { useZoneGames } from "./use-zone-games"
import { ZoneGround } from "./ui/organisms/ZoneGround"

/**
 * The composition root: the only file that reads the treaty's model and
 * host. It runs the home program (useZoneGames: the navigation, the host's
 * buttons, idle) and mounts the GAMES page over the ground. GAMES is home:
 * the zone select a person sees on power-on, the first card focused.
 *
 * The page draws OPTIONS and SAVE DATA as placeholders until their pages
 * land; the navigation's `tab` already drives which one shows.
 */
export function ZoneSurface({ model, host }: { readonly model: SurfaceModel; readonly host: SurfaceHost }) {
  const root = useRef<HTMLDivElement | null>(null)
  const { nav, page } = useZoneGames(model, host, root)
  return (
    <div className="zone-theme zone-screen" data-layer={nav.layers.at(-1) ?? "none"} ref={root}>
      <ZoneGround />
      <ZoneGames {...page} />
    </div>
  )
}
