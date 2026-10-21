/**
 * Browser-dev seeds for launch settings, so every editor state can be reached
 * in `portal-dev`. Pick one with `?launch=`:
 *
 *   (none)     no device facts, as a browser has: 10 % steps, "clock steps unknown"
 *   rg353m     the RG353M's real CPU and GPU clock steps, with a runner, a game
 *              value and a profile already set on Neverball
 *   needs-runner  Neverball has no runner chosen: Device, Game and profiles only
 *   unavailable   Neverball's route cannot be read
 *   conflict   every write is refused as changed elsewhere
 *   error      the read fails
 *
 * Shapes are the generated wire types; the values are made up.
 */
import type { InMemoryLaunchSettingsConfig } from "./in-memory-launch-settings"
import { RG353M_LAUNCH_FACTS } from "./in-memory-launch-settings"

const games: InMemoryLaunchSettingsConfig["games"] = {
  neverball: { systemId: "linux", runner: { id: "neverball/native", title: "Neverball" } },
  skate3: { systemId: "ps3", runner: { id: "rpcs3/linux", title: "RPCS3" }, releaseId: "BLUS30464" },
}

export function inMemoryLaunchSettingsDevConfig(scenario: string | null): InMemoryLaunchSettingsConfig {
  switch (scenario) {
    case "rg353m":
      return {
        games,
        deviceFacts: RG353M_LAUNCH_FACTS,
        layers: {
          device: { cpu: { max: 89 }, gpu: {} },
          "runner:neverball/native": { cpu: { min: 33 }, gpu: {} },
          "game:neverball": { cpu: {}, gpu: { max: 75 } },
        },
        profiles: { neverball: [{ id: "battery", title: "Battery saver", cpu: { max: 61 }, gpu: {} }] },
        selected: { neverball: "battery" },
      }
    case "needs-runner":
      return { games: { ...games, neverball: { routeLayers: { _tag: "NeedsRunnerChoice" } } }, deviceFacts: RG353M_LAUNCH_FACTS }
    case "unavailable":
      return {
        games: { ...games, neverball: { routeLayers: { _tag: "Unavailable", message: "The runner plugin did not load" } } },
        deviceFacts: RG353M_LAUNCH_FACTS,
      }
    case "conflict":
      return { games, failures: { set: { code: "SettingsConflict", message: "Launch settings changed." }, profile: { code: "SettingsConflict", message: "Profiles changed." } } }
    case "error":
      return { games, failures: { snapshot: { code: "LaunchSettingsUnavailable", message: "Korri could not read launch settings." } } }
    default:
      return { games }
  }
}
