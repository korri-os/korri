import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"

/** Presentation-only fixture grounded in the GameRoutes-derived surface treaty. */
export const runnerChoice: Exclude<SurfaceRunnerChoice, { _tag: "Closed" }> = {
  _tag: "Stale",
  gameTitle: "Wario Land 4",
  message: "A saved runner is not installed. The choice is still saved.",
  saved: [{ label: "This game", runnerId: "missing/runner" }],
  warnings: [],
  routes: ["retroarch/mgba", "retroarch/mgba-nightly"].map((runnerId, index) => ({
    runnerId,
    familyId: "@korri:retroarch",
    systemId: "gba",
    runnerBuild: `/nix/store/exact-${runnerId.replaceAll("/", "-")}`,
    // Each runner ships its own frontend build inside its own closure.
    program: `/nix/store/exact-${runnerId.replaceAll("/", "-")}/bin/retroarch`,
    warnings: [],
    actions: [
      { id: `runner:launch:${index}`, label: "Launch once", enabled: true },
      { id: `runner:game:${index}`, label: "Remember for this game", enabled: true },
      { id: `runner:system:${index}`, label: "Remember for system gba", enabled: true },
    ],
  })),
  actions: [
    { id: "runner:clear-game", label: "Clear game choice", enabled: true },
    { id: "runner:cancel", label: "Cancel", enabled: true },
  ],
}
