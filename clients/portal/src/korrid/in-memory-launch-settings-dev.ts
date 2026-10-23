/**
 * Browser-dev seeds for launch settings, so every editor state can be reached
 * in `portal-dev`. Pick one with `?launch=`:
 *
 *   (none)        no device facts, as a browser has: 10 % steps, "clock steps unknown"
 *   rg353m        the RG353M's real CPU and GPU clock steps; Neverball with two
 *                 runners of one family, runner settings, a profile, hooks and env;
 *                 Gamescope on for the device and off in the profile, and
 *                 MangoHud, which has no settings
 *   needs-runner  Neverball's two runners and no runner named anywhere
 *   missing-runner Neverball's game place names a runner that is not installed
 *   facts-unavailable the native runner's dry run fails
 *   unavailable   Neverball's route cannot be read
 *   conflict      every write is refused as changed elsewhere
 *   error         the read fails
 *
 * Shapes are the generated wire types; the values are made up.
 */
import type { InMemoryLaunchRunner, InMemoryLaunchSettingsConfig } from "./in-memory-launch-settings"
import { RG353M_LAUNCH_FACTS } from "./in-memory-launch-settings"

const NEVERBALL_SCHEMA = {
  type: "object",
  properties: {
    fullscreen: { type: "boolean", title: "Fullscreen", default: true },
    fps: { type: "integer", title: "Frame limit", description: "Frames per second; 0 is no limit.", minimum: 0, maximum: 240, default: 60 },
    renderer: { type: "string", title: "Renderer", enum: ["gl", "gles", "software"], default: "gl" },
    lang: { type: "string", title: "Language", "x-korri-scope": "device" },
  },
}

const runners: Readonly<Record<string, InMemoryLaunchRunner>> = {
  "neverball/native": {
    title: "Neverball",
    family: "neverball",
    schema: NEVERBALL_SCHEMA,
    prepare: { _tag: "Ready", payload: { env: { SDL_VIDEODRIVER: "kmsdrm" }, args: ["--data", "/usr/share/neverball"] } },
  },
  "neverball/flatpak": {
    title: "Neverball (Flatpak)",
    family: "neverball",
    schema: NEVERBALL_SCHEMA,
    prepare: { _tag: "Ready", payload: { env: {}, args: ["run", "org.neverball.Neverball"], cwd: "/var/lib/flatpak" } },
  },
  "rpcs3/linux": { title: "RPCS3", family: "rpcs3" },
}

/** Gamescope with a few of its options, and MangoHud, which describes none. */
const modifiers: InMemoryLaunchSettingsConfig["modifiers"] = {
  "korri/gamescope": {
    title: "Gamescope",
    schema: {
      type: "object",
      properties: {
        "output-width": { type: "integer", title: "Output width", minimum: 320, maximum: 7680 },
        "output-height": { type: "integer", title: "Output height", minimum: 240, maximum: 4320 },
        filter: { type: "string", title: "Upscale filter", enum: ["linear", "nearest", "fsr", "nis"] },
      },
    },
  },
  "korri/mangohud": { title: "MangoHud" },
}

const games: InMemoryLaunchSettingsConfig["games"] = {
  neverball: { systemId: "linux", runners: ["neverball/native", "neverball/flatpak"] },
  skate3: { systemId: "ps3", runners: ["rpcs3/linux"], releaseId: "BLUS30464" },
}

const chosen = { "game:neverball/any": { runner: "neverball/native" } }

export function inMemoryLaunchSettingsDevConfig(scenario: string | null): InMemoryLaunchSettingsConfig {
  switch (scenario) {
    case "rg353m":
      return {
        games,
        runners,
        modifiers,
        deviceFacts: RG353M_LAUNCH_FACTS,
        bodies: {
          "device/any": { cpu: { max: 89 }, hooks: { use: ["fan-quiet"] }, modifiers: { "korri/gamescope": { enabled: true, settings: { "output-width": 1280 } } } },
          "device/runner:neverball/native": { cpu: { min: 33 }, settings: { fullscreen: true } },
          "game:neverball/any": { runner: "neverball/native", gpu: { max: 75 }, env: { SDL_AUDIODRIVER: "pipewire", MESA_DEBUG: null }, args: { append: ["--verbose"] } },
          "game:neverball/runner:neverball/native": { settings: { fps: 30 } },
          "game:neverball/family:neverball": { hooks: { before: [{ run: "notify-send 'Rolling'" }] } },
          "profile:battery/any": { cpu: { max: 61 }, modifiers: { "korri/gamescope": { enabled: false } } },
        },
        profiles: { neverball: [{ id: "battery", title: "Battery saver" }] },
        selected: { neverball: "battery" },
      }
    case "needs-runner":
      return { games, runners, deviceFacts: RG353M_LAUNCH_FACTS }
    case "missing-runner":
      return {
        games,
        runners,
        deviceFacts: RG353M_LAUNCH_FACTS,
        bodies: { "device/any": { runner: "neverball/flatpak" }, "game:neverball/any": { runner: "neverball/snap" } },
      }
    case "facts-unavailable":
      return {
        games,
        runners: {
          ...runners,
          "neverball/native": { ...runners["neverball/native"], title: "Neverball", prepare: { _tag: "Unavailable", payload: "launch.prepare timed out after 2 s" } },
        },
        deviceFacts: RG353M_LAUNCH_FACTS,
        bodies: chosen,
      }
    case "unavailable":
      return {
        games: { ...games, neverball: { runners: [], routeLayers: { _tag: "Unavailable", message: "The runner plugin did not load" } } },
        runners,
        deviceFacts: RG353M_LAUNCH_FACTS,
      }
    case "conflict":
      return { games, runners, bodies: chosen, failures: { set: { code: "SettingsConflict", message: "Launch settings changed." }, profile: { code: "SettingsConflict", message: "Profiles changed." } } }
    case "error":
      return { games, runners, failures: { snapshot: { code: "LaunchSettingsUnavailable", message: "Korri could not read launch settings." } } }
    default:
      return { games, runners, modifiers, bodies: chosen }
  }
}
