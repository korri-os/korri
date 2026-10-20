/**
 * Hold Zone's moments at one frame for a part's still: Caliper takes its
 * picture whenever the page has loaded, and a moment caught mid-move would
 * be a different picture each run. A part that shows a moment calls this
 * with the time it wants (as `window.seek` would) and the frame stands.
 * Fixture code only: the product never holds its clock.
 */
import { useLayoutEffect } from "react"
import { playZoneMoments, seekZoneMoments } from "../use-zone-moment"

export function useFixtureHold(at: number): void {
  useLayoutEffect(() => {
    seekZoneMoments(at)
    return () => playZoneMoments()
  }, [at])
}
