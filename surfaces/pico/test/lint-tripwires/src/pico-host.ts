// Tripwire: the door may call the host. Nothing here may be reported.
import type { SurfaceHost } from "@contracts/surface/korri-surface"

export const launch = (host: SurfaceHost): void => host.launchGame("tetris")
