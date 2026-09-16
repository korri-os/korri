import type { GameRoutes } from "@contracts/generated/korrid"

/** GameRoutes wire fixture; two installed claims for the same system. */
export const runnerRoutes: GameRoutes = {
  gameId: "wl4",
  selection: { _tag: "Choose" },
  gameRunner: "missing/runner",
  systemRunners: { gba: "retroarch/mgba" },
  revisions: { games: "g1", device: "d1" },
  routes: ["retroarch/mgba", "retroarch/mgba-nightly"].map(runnerId => ({
    runnerId,
    familyId: "@korri:retroarch",
    systemId: "gba",
    runnerBuild: `/nix/store/0123456789abcdef0123456789abcdef-${runnerId.replaceAll("/", "-")}`,
    program: "/nix/store/fedcba9876543210fedcba9876543210-retroarch/bin/retroarch",
    warnings: [],
  })),
}
