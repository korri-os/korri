// Tripwire: host commands and host input outside src/pico-host.ts (rule 2).
import type { SurfaceHost } from "@contracts/surface/korri-surface"

export const launch = (host: SurfaceHost): void => host.launchGame("tetris") // VIOLATION no-restricted-properties
export const listen = (host: SurfaceHost): (() => void) => host.input.on("back", () => undefined) // VIOLATION no-restricted-properties
export const destructured = (host: SurfaceHost): void => {
  const { runAction } = host // VIOLATION no-restricted-properties
  runAction("a")
}
export const reads = (host: SurfaceHost): number => host.gameActions("tetris").length
