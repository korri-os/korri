import type { GameRoutes } from "@contracts/generated/korrid"

/** GameRoutes wire fixture; two installed claims for the same system. */
export const runtimeRoutes: GameRoutes = {
  gameId: "wl4",
  selection: { _tag: "Choose" },
  gameRuntime: "missing/runtime",
  systemRuntimes: { gba: "retroarch/mgba" },
  revisions: { games: "g1", device: "d1" },
  routes: ["retroarch/mgba", "retroarch/mgba-nightly"].map(runtimeId => ({
    runtimeId,
    launcherId: "retroarch/linux",
    launcherKind: "retroarch",
    systemId: "gba",
    runtimeBuild: `/nix/store/0123456789abcdef0123456789abcdef-${runtimeId.replaceAll("/", "-")}`,
    launcherBuild: "/nix/store/fedcba9876543210fedcba9876543210-retroarch-linux",
    program: "/nix/store/fedcba9876543210fedcba9876543210-retroarch/bin/retroarch",
    warnings: [],
  })),
}
