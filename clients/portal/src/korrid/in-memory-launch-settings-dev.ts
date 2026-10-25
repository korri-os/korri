/**
 * Browser-dev seeds for launch settings, so every editor state can be reached
 * in `portal-dev`. Pick one with `?launch=`:
 *
 *   (none)        no device facts, as a browser has: 10 % steps, "clock steps unknown"
 *   rg353m        the RG353M's real CPU and GPU clock steps; Neverball with two
 *                 runners of one family, runner settings by section, a profile,
 *                 hooks and env; Gamescope on for the device and off in the
 *                 profile, and MangoHud, which has no settings; Moonlight's
 *                 settings (a stream runner, never a game's runner) and
 *                 Sunshine's (a launch service), one cleared to Default and
 *                 one korrid skips
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

const section = (title: string, properties: Record<string, unknown>) => ({ type: "object", title, additionalProperties: false, properties })
const pair = (title: string, low: number, high: number) => section(title, {
  width: { type: "integer", title: "Width", minimum: low, maximum: high },
  height: { type: "integer", title: "Height", minimum: low, maximum: high },
})

const NEVERBALL_SCHEMA = {
  type: "object",
  additionalProperties: false,
  properties: {
    video: section("Video", {
      fullscreen: { type: "boolean", title: "Fullscreen", default: true },
      "fps-limit": { type: "integer", title: "FPS limit", description: "Frames per second; 0 is no limit.", minimum: 0, maximum: 240, default: 60 },
      renderer: { type: "string", title: "Renderer", enum: ["gl", "gles", "software"], default: "gl" },
    }),
    audio: section("Audio", { enable: { type: "boolean", title: "Sound", default: true } }),
    locale: section("Language", { language: { type: "string", title: "Language", "x-korri-scope": "device" } }),
  },
}

/** Moonlight's settings, after docs/briefs/2026-10-25-settings-vocabulary.md. */
const MOONLIGHT_SCHEMA = {
  type: "object",
  additionalProperties: false,
  properties: {
    video: section("Video", {
      resolution: pair("Resolution", 320, 7680),
      fps: { type: "integer", title: "FPS", minimum: 10, maximum: 240 },
      "bitrate-kbps": { type: "integer", title: "Bitrate (kbps)", minimum: 500, maximum: 150000 },
      codec: { type: "string", title: "Codec", enum: ["auto", "h264", "h265", "av1"], default: "h264" },
      hdr: section("HDR", { enable: { type: "boolean", title: "Enable" } }),
      "rotation-degrees": { type: "integer", title: "Rotation", enum: [0, 90, 180, 270] },
    }),
    audio: section("Audio", {
      channels: { type: "string", title: "Channels", enum: ["stereo", "5.1", "7.1"] },
      "play-on-host": { type: "boolean", title: "Play on host" },
    }),
    input: section("Input", { "mouse-emulation": { type: "boolean", title: "Mouse emulation" } }),
    network: section("Network", {
      "remote-optimizations": { type: "string", title: "Remote optimizations", enum: ["auto", "on", "off"] },
      "packet-size-bytes": { type: "integer", title: "Packet size (bytes)", minimum: 256, maximum: 65536 },
    }),
    logging: section("Logging", { level: { type: "string", title: "Level", enum: ["info", "verbose", "debug"] } }),
  },
}

/** Some of Sunshine's settings, after the same brief. */
const SUNSHINE_SCHEMA = {
  type: "object",
  additionalProperties: false,
  properties: {
    video: section("Video", {
      "max-bitrate-kbps": { type: "integer", title: "Max bitrate (kbps)", minimum: 0, maximum: 150000 },
      "minimum-fps": { type: "integer", title: "Minimum FPS", minimum: 0, maximum: 240 },
      encoder: section("Encoder", {
        name: { type: "string", title: "Encoder", enum: ["v4l2m2m", "rkmpp", "software"] },
        "fec-percent": { type: "integer", title: "FEC (%)", minimum: 1, maximum: 255, default: 20 },
      }),
    }),
    audio: section("Audio", { enable: { type: "boolean", title: "Stream audio", default: true } }),
    input: section("Input", {
      gamepad: { type: "string", title: "Gamepad", enum: ["auto", "ds5", "switch", "xone"], default: "auto" },
      mouse: { type: "boolean", title: "Mouse" },
      keyboard: { type: "boolean", title: "Keyboard" },
    }),
    network: section("Network", {
      "lan-encryption": { type: "string", title: "LAN encryption", enum: ["off", "supported", "required"] },
      "external-ip": { type: "string", title: "External IP" },
    }),
    logging: section("Logging", { level: { type: "string", title: "Level", enum: ["verbose", "debug", "info", "warning", "error", "fatal", "none"], default: "info" } }),
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
  "@korri:moonlight/moonlight": { title: "Moonlight", schema: MOONLIGHT_SCHEMA, choosable: false },
}

const services: InMemoryLaunchSettingsConfig["services"] = {
  "@korri:sunshine/sunshine": { title: "Sunshine", schema: SUNSHINE_SCHEMA },
}

/** Gamescope with a few of its options, and MangoHud, which describes none. */
const modifiers: InMemoryLaunchSettingsConfig["modifiers"] = {
  "korri/gamescope": {
    title: "Gamescope",
    schema: {
      type: "object",
      additionalProperties: false,
      properties: {
        video: section("Video", {
          output: pair("Output", 240, 7680),
          "fps-limit": { type: "integer", title: "FPS limit", minimum: 0, maximum: 240 },
          scaling: section("Scaling", {
            scaler: { type: "string", title: "Scaler", enum: ["auto", "integer", "fit", "fill", "stretch"] },
            filter: { type: "string", title: "Filter", enum: ["linear", "nearest", "fsr", "nis", "pixel"] },
          }),
        }),
        window: section("Window", { borderless: { type: "boolean", title: "Borderless" } }),
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
        services,
        routines: { "night-mode": { before: [{ run: "brightnessctl set 30%" }], after: [{ run: "brightnessctl set 100%" }] } },
        deviceFacts: RG353M_LAUNCH_FACTS,
        bodies: {
          "device/any": {
            cpu: { max: 89 },
            hooks: { use: ["night-mode"] },
            modifiers: { "korri/gamescope": { enabled: true, settings: { video: { output: { width: 1280, height: 720 } } } } },
            services: { "@korri:sunshine/sunshine": { settings: { video: { "max-bitrate-kbps": 3000, encoder: { name: "v4l2m2m" } }, input: { mouse: false } } } },
          },
          "device/runner:neverball/native": { cpu: { min: 33 }, settings: { video: { fullscreen: true } } },
          // Moonlight on the Device: 30 fps, and its codec cleared to Moonlight's default.
          "device/runner:@korri:moonlight/moonlight": { settings: { video: { fps: 30, codec: null, "bitrate-kbps": 5000 } } },
          "game:neverball/runner:@korri:moonlight/moonlight": { settings: { video: { resolution: { width: 854, height: 480 } } } },
          "game:neverball/any": { runner: "neverball/native", gpu: { max: 75 }, env: { SDL_AUDIODRIVER: "pipewire", MESA_DEBUG: null }, args: { append: ["--verbose"] } },
          // 300 is above Neverball's 240: korrid skips it and warns.
          "game:neverball/runner:neverball/native": { settings: { video: { "fps-limit": 300, renderer: null } } },
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
      return { games, runners, modifiers, services, bodies: chosen }
  }
}
