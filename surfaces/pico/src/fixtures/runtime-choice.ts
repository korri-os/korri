import type { SurfaceRuntimeChoice } from "@contracts/surface/korri-surface"

/** Presentation-only fixture grounded in the GameRoutes-derived surface treaty. */
export const runtimeChoice: Exclude<SurfaceRuntimeChoice, { _tag: "Closed" }> = {
  _tag: "Stale",
  gameTitle: "Wario Land 4",
  message: "A saved runtime is not installed. The choice is still saved.",
  saved: [{ label: "This game", runtimeId: "missing/runtime" }],
  warnings: [],
  routes: ["retroarch/mgba", "retroarch/mgba-nightly"].map((runtimeId, index) => ({
    runtimeId,
    launcherId: "retroarch/linux",
    launcherKind: "retroarch",
    systemId: "gba",
    runtimeBuild: `/nix/store/exact-${runtimeId.replaceAll("/", "-")}`,
    launcherBuild: "/nix/store/exact-retroarch-build",
    program: "/nix/store/exact-retroarch-build/bin/retroarch",
    warnings: [],
    actions: [
      { id: `runtime:launch:${index}`, label: "Launch once", enabled: true },
      { id: `runtime:game:${index}`, label: "Remember for this game", enabled: true },
      { id: `runtime:system:${index}`, label: "Remember for system gba", enabled: true },
    ],
  })),
  actions: [
    { id: "runtime:clear-game", label: "Clear game choice", enabled: true },
    { id: "runtime:cancel", label: "Cancel", enabled: true },
  ],
}
