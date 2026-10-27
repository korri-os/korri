import type { Game, LocalGame } from "@contracts/generated/korrid"

// Launch fields from rpminiv2's app.catalog.snapshot and app.local-games.list,
// captured in /tmp/sp-proof/{cat2,local}.json on 2026-10-27.
export const turtlesLocalGame: LocalGame = {
  id: "teenage-mutant-ninja-turtles-iv-turtles-in-time-usa",
  title: "Teenage Mutant Ninja Turtles IV - Turtles in Time (USA)",
  system: "Super Nintendo",
  identity: { kind: "hash", value: "sha256:5b82cdd6f2da56f43680d6a5021faebe2e06036d30602c1a7917aa414cf8b5f4" },
}

export const turtlesCatalogGames: readonly Game[] = ["rpminiv2", "rg35xxsp", "odin2portal"].map(host => ({
  id: turtlesLocalGame.id,
  title: turtlesLocalGame.title,
  identity: turtlesLocalGame.identity,
  host,
  source: { label: host, isLocal: host === "rpminiv2" },
  supportsRunnerSelection: true,
}))
