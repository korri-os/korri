# Overlay: local viewer facts and stream controls (#378, #379)

Status: **Phase 1 proposal only. No implementation.**
Source inspected: `36f645eac2c03bad36612c1eb1f9e2e48204efe7` (current main,
including #376 and #377). Line numbers below refer to that revision.
`legacy:` citations refer to read-only `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62`.

The phone's korrid combines its **local** viewer observations with the exact
streamed session's title and plugin controls. Pico and Zone receive facts and the
existing gameplay-control vocabulary, never Android operations. The strip
exists only with the gameplay overlay and has no focus targets.

## Source key

Paths below are relative to the repository; these abbreviations keep the
inventory readable:

- `G`: `clients/android/app/src/main/java/com/limelight/Game.java`
- `B`: `clients/android/app/src/main/java/com/limelight/KorriSettingsBridge.java`
- `O`: `clients/android/app/src/main/java/com/limelight/KorriGameOverlay.java`
- `H`: `clients/android/app/src/main/assets/korri-shell/overlay.html`
- `P`: `clients/android/app/src/main/java/com/limelight/preferences/PreferenceConfiguration.java`
- `D`: `clients/android/app/src/main/java/com/limelight/binding/video/MediaCodecDecoderRenderer.java`
- `V`: `clients/android/app/src/main/java/com/limelight/KorriStreamViewer.java`
- `E`: `clients/android/app/src/main/java/com/limelight/korri/moonlight/KorriMoonlightActionExecutor.java`
- `T`: `contracts/surface/src/lib.rs`

## Status-strip inventory

| Fact | Current producer, value and limits | Locality / gap |
|---|---|---|
| Time | `services/korrid/src/surface/root.rs:257–272`: `clock_label()` returns a local-time `String`, `h:mm AM/PM`, hour 1–12, minute 00–59; empty on conversion failure. Refresh at `:900–913`. Existing `SurfaceModel.clock_label: Option<String>` (`T:1364–1366`). | Phone's clock, not aka's. No touch dependency. Overlay currently loses it: `surface/overlay.rs:192` sets `None`, and `surface/mod.rs:305–308` returns that model instead of root facts. Reuse the root clock, not another clock field. |
| Phone battery | `clients/android/app/src/main/java/com/simonwjackson/korri/korrid/KorridNetwork.java:53–56` already reads Android's sticky `ACTION_BATTERY_CHANGED`, but **only** exposes `EXTRA_PLUGGED > 0` as `external_power: bool`. There is no phone battery percentage producer in current Korri. Extract percentage from Android `BatteryManager.BATTERY_PROPERTY_CAPACITY` (integer percent 0–100; unsupported/error is not a percentage). API source: [BatteryManager](https://developer.android.com/reference/android/os/BatteryManager#BATTERY_PROPERTY_CAPACITY), [getIntProperty](https://developer.android.com/reference/android/os/BatteryManager#getIntProperty(int)). | Battery-capable local device only, not controller battery (`ControllerHandler.java:1132–1165`). No touch dependency. Read the Android property at the viewer boundary; do not infer percentage from external power. |
| Stream health | RTT: `D:1800,1849–1850`, `MoonBridge.java:505–506`; native `clients/android/app/src/main/jni/moonlight-core/simplejni.c:295–303` packs two unsigned 32-bit integers, RTT and variance in ms, into a Java `long`; `-1` means unavailable. Network-lost frames: `VideoStats.java:9–13`, `D:1761–1768,1834–1835`: integer `framesLost / totalFrames * 100`, normally 0–100%, over the last two windows. Display drops: existing `frameDropped` decisions at `D:1274–1463`, not a published counter. Current bandwidth: `D:1803–1814,1836–1847` uses UID **RX+TX byte deltas**, a Java `float` displayed as K/s or M/s. It is not stream video bitrate and assumes roughly one-second sampling. | Local decoder/connection only; no touch dependency. Stats windows turn over roughly each second (`D:1780–1785,1864–1867`), but structured publication does not exist. Formatting/callback is gated by `enablePerfOverlay`; #378 must not enable Moonlight's HUD to obtain facts. See measurement correction below. |
| Game name | `services/korrid/src/lib.rs:635–641`: exact `ActiveSession.title: Option<String>`; `SessionControls.title` at `:372–376` already reaches `SurfaceGameplayOverlayPresentation.title` through `surface/overlay.rs:150–164`. Arbitrary string; no numeric range or length restriction in those types. | Streamed session fact, already available locally. **Not** `G.EXTRA_APP_NAME`: `V:324–337,357–365` selects Sunshine's transport app, “Korri Stream”, not the game. No new title producer or treaty field. |

### Measurement correction before implementation

“Moonlight measures these” does not mean all three are ready as structured
facts. Do not parse `onPerfUpdate(String)` (`G:3950–3959`), label UID traffic
as stream bitrate, label configured `prefConfig.bitrate` as measured bitrate,
or label network-lost frames as locally dropped display frames.

Use RTT directly. Count encoded video bytes entering `submitDecodeUnit`
(`D:1753–1755`, `decodeUnitLength`) over actual monotonic elapsed time, and
count the existing display-discard decisions once per discarded frame. Both
need instrumentation, not new estimates. Encoded video bitrate excludes audio,
protocol overhead and undecodable/lost video; call it video bitrate.

Grounding exists in read-only legacy, rather than a new telemetry schema:

- `legacy:product/platform/stream/stream-health.ts:1–14` already names
  `rttMs`, `deliveredBitrateKbps` and `framesDropped`, each optional number.
- `legacy:product/plugins/moonlight/packages/moonlight-embedded-korri/patches/0016-add-stream-health-sampling.patch:227–233,350–384`
  counts dropped display frames and computes delivered video kbps as
  `encoded_bytes * 8 / elapsed_ms`. `framesDropped` is a **sample-interval
  count**, not a lifetime total. Preserve those meanings.
- `legacy:product/platform/device/device-facts.ts:16–23,252–255` provides
  nullable numeric `percent`, rounded and bounded to 0–100.

No threshold-based “good/bad” health grade is proposed. Failed/missing RTT,
no bitrate interval yet, and no drop observation are unavailable, not zero.
A measured interval with no dropped frames is zero. A stopped or replaced
viewer must not leave its last sample looking live. Retain legacy's 3,000 ms
stale cutoff (`stream-health.ts:58,60–66`) internally and clear the projection
when it expires; use monotonic elapsed time on this device. No smoothing or
new timeout default. Sample ownership and freshness stay in korrid; surfaces
get only the current valid projection.

## Stream-control inventory and first cut

The old page has **15 top-level controls**, plus local cursor inside Mouse
mode (`H:63–103,144–160`). The table records the actual runtime operation,
not the wider settings screen. “Include / touch” means declare only when the
local viewer has touchscreen input. It does not mean show a disabled row on
a device without touch. Controller/keyboard navigation of the overlay is not
itself evidence that a touch-only effect is useful.

| Control | Current producer and value/range | First cut and device restriction |
|---|---|---|
| Screen fit | `O:226–228,319–321`; `G:4007–4038`: boolean fill; true = Fill, false = not Fill. Enabling then disabling selects Fit. `P:570–585` also supports a persisted Stretch baseline; `B:84–86` offers all three strings, but the live overlay only toggles fill. | **Include**, `Toggle`, existing `E:27` set-fill-mode. Describe it as “Crop to fill”, On/Off, so false does not mislabel a Stretch baseline as Fit. No new three-way live operation. Not touch-only. |
| Toggle keyboard | `O:218–223`; `G:2372–2375`: parameterless IME toggle, no reliable readback here. | **Include / touch**, `Command`. Android IME is the current adapter, not a treaty variant. Resume/release the input hold before showing it. |
| Full keyboard | `O:268–273`; `G:1236–1260`: parameterless show/hide of `KeyBoardLayoutController`; nullable controller, `shown: bool`. | **Include / touch**, `Command` as `E:26` already declares. This is a stream-input keyboard, not a second Korri overlay. Resume before using it. |
| Pan & zoom | `O:231–242`; `G:184,4041–4062`: boolean `isPanZoomMode`, initially false; finger handling at `G:3042–3049`. | **Include / touch**, `Toggle` (existing `E:28`). Resume after success. Changing mouse mode turns this off (`G:4195–4197`); republish both values. |
| Mouse mode | `O:292–316`; `G:4156–4197,4408–4415`; `res/values/arrays.xml:124–139`: string choices `"0"` multi-touch, `"1"` absolute, `"2"` natural trackpad, `"3"` gaming trackpad, `"4"` disabled, `"5"` absolute with swapped buttons. Runtime accepts integer 0–5; `E:234–237` validates the string form. | **Include / touch**, `Choice`, only when `allowChangeMouseMode` permits (`G:4375–4378`). These are touch-to-mouse modes, not a generic physical-mouse toggle. Keep labels from the viewer. |
| Local cursor (Mouse submenu) | `O:302–304`; `G:183,4141–4153,4418–4420`: `cursorVisible: bool`; old menu uses sentinel index `-1` to toggle it. | **Include conditionally**, `Command` as existing `E:35`; useful with a local pointing device, not inherently touchscreen-only. Keep it separate from the six choices; `-1` is not a mode. Declare only if input capture/cursor is supported. |
| Rotate screen | `O:244–249`; `G:4065–4072`: parameterless switch between portrait and landscape Android orientation constants. | **Include conditionally**, `Command`. Viewer must support rotation; not touchscreen-only. No Android constant crosses the treaty. |
| HUD | `O:252–257`; `G:4200–4211`: parameterless toggle of `prefConfig.enablePerfOverlay: bool`. Startup preference default false (`P:151,917`). | **Drop** from first cut: duplicate stats presentation drawn by Moonlight, not the active surface. #378 supplies the status strip. Cost: no persistent in-stream diagnostic panel. Strip remains overlay-only; this is not a hidden way to keep it open. |
| Floating menu button | `O:260–265`; `G:4602–4605`: toggles Android View visibility (VISIBLE/GONE), not the persisted flag. Touch drag/click handler at `G:1203–1220`. Preference default false (`P:180,931`). | **Drop** from stream controls: it is an overlay entry mechanism, not stream presentation/input configuration. Do not create a second Korri menu. Cost: touch-only access to the overlay still needs an entry mechanism; #394's taps inside the overlay do not by themselves solve entry from gameplay. |
| Keyboard as controller | `O:276–281`; `G:1230–1233,1247–1252`: parameterless show/hide of `KeyBoardController`; initially absent. | **Include / touch**, `Command` (`E:32`). It is a virtual stream-input control panel. Resume before using it; do not confuse it with a physical-keyboard mapping setting. |
| Touch sensitivity | `O:284–289`; `G:4215–4216`: toggles `prefConfig.enableTouchSensitivity: bool`; consumer at `G:2473–2476`. Startup false, X/Y multipliers initially 100 (`P:955–963`). | **Include / touch**, `Command`, matching `E:33`. The menu does **not** adjust a numeric sensitivity. Do not manufacture a slider/range from the unrelated trackpad X/Y settings in `B:132–133`. |
| SGSR sharpness | `H:83–88`; `B:95`; `G:4424–4433,4577–4582`: integer 0–50, step 1, baseline 20; renderer value = integer / 10 (0–5). | **Include conditionally**, `Range`, only while the SGSR renderer is active (render mode 3; `B:92–95`). Not touch-only; omit when it cannot affect the live stream. |
| SGSR edge threshold | `H:89–93`; `B:96`; `G:4437–4446,4577–4582`: integer 1–32, step 1, baseline 8; renderer value = integer / 255. | **Include conditionally**, `Range`, SGSR only; not touch-only. Preserve raw range units, no new normalized range. |
| Flip A/B and X/Y | `H:95–97`; `B:124`; `G:4450–4458,4584`: boolean, baseline false. | **Include**, `Toggle`, for controller input. Not touch-only. Do not expose an Android preference key to the surface. |
| Rumble | `H:98–100`; `B:122`; `G:4462–4470,4585`: boolean, baseline true. | **Include conditionally**, `Toggle`, when this viewer can deliver controller rumble. Not touch-only; distinct from “rumble on this device” (`B:129`), which the old page does not offer. |
| Picture-in-picture | `H:101–103`; `B:102`; `G:4474–4483,4586`: boolean enable flag, baseline false; not an “enter PiP now” command. Availability at `G:4381–4384`: Android O+ and `FEATURE_PICTURE_IN_PICTURE`. | **Include conditionally**, `Toggle`, when the platform provides PiP. Not touch-only. Apply `updatePipAutoEnter()` too; `O:331–334` currently only calls `applyLivePrefs()`. |

Defaults mean the existing stream-start baseline, not new factory values.
`G:365` reads `PreferenceConfiguration`; `G:4085–4095` reads the mouse-mode
baseline (`"0"` absent a saved setting). Existing settings may change that
baseline. No new persisted schema, overrides file or settings cascade belongs
in this cut.

**Session-only is a real change:** `B:171–200` writes SharedPreferences;
`G:4430–4482` calls it for SGSR/flip/rumble/PiP; `G:4561–4565` optionally
persists mouse mode. Do not reuse these setters unchanged. Apply to the
current in-memory config/renderer/controller and read back that same runtime
state. Keep the current stream's changes with `V.Session`, across Activity
recreation or decoder reattachment; discard them when that stream ends.
The next `V.start` creates a new Session (`V:135–146`) from the unchanged
baseline. Do not “restore” preferences on teardown: that would race unrelated
settings writes. `applyLivePrefs()` must not overwrite stream-local changes.

## Path: viewer → local korrid → surface → viewer

1. Extend the **existing exact-launch viewer adapter**, not the Slint Surface
   attachment: `services/korrid/src/linux_viewer.rs:33–63`,
   `services/korrid/src/android/viewer.rs:19–67,109–188`, and `V`.
   The current trait's Linux name predates Android; keep the extension about
   viewer facts and controls, not a general platform capability model.
2. Pull a typed snapshot and apply a typed control through JNI on a worker,
   using the existing static Java call / bounded local-frame pattern in
   `android/viewer.rs`. Java marshals UI operations to the main thread.
   Snapshot includes only the current supported controls and runtime values,
   battery reading and latest structured health sample. The exact launch ID,
   viewer generation and Activity lease already exist (`V:41–60,256–259,369–383`;
   `KorriStreamViewerState.java:14–32,54–59`). Recheck these before the final
   effect, and reject stale results after a replacement. No Activity address,
   certificate, Java object or input code enters the surface model.
3. Reuse `SurfaceGameplayControlGroup` for a group labelled “Stream controls”.
   Its IDs are opaque just like plugin IDs. korrid associates them with the
   current local viewer's declared operations; the surface never routes an
   effect by parsing an ID. Viewer-only rows do not depend on aka answering
   plugin-control discovery. No static Android checklist in Pico or Zone.
4. Inject this local fact/effect reader into the overlay controller, following
   the internal fact-reader precedent in `surface/korrid.rs:1–7,35–51`.
   `OverlayController.refresh` currently reads streamed session status and plugin
   controls (`surface/overlay.rs:388–497`); merge local observations without
   moving them to the streamed session. A roughly one-second local refresh while
   the overlay is open follows the decoder's existing cadence. It must not
   poll aka for every stats sample or wait for plugin discovery before showing
   local facts. Clear failed/stale observations, cancel on overlay destruction,
   and notify the surface hub when the current projection changes. Keep
   sampling independent of `enablePerfOverlay`; do not open `overlay.html`.
5. **RPC reuse:** packages continue to use `app.surface.model`,
   `app.surface.watch`, and `app.surface.request` with
   `InvokeGameplayControl { control_id, value }` (`T:1495–1504`;
   `surface/mod.rs:69–98,133–140`). No new publication RPC is needed for this
   pull path. `Root.perform` already delivers requests to `OverlayController`
   (`surface/root.rs:1513–1525`). Add local-viewer dispatch there alongside
   core/plugin dispatch, before `invoke_plugin` sends anything to the streamed
   session. Reuse existing value validation and failure vocabulary
   (`services/korrid/src/lib.rs:394–430`). Do **not** send local operations to
   `app.session.control.invoke`: that path controls the streamed game's plugin
   and enforces its authority. A watching person may change their own viewer
   without gaining permission to change the game.
6. Republish observed runtime values after success, including coupled changes
   (mouse mode disables pan & zoom). Errors stay visible, not optimistic
   success. Honor `dismiss_on_success`: stream-input commands resume/release
   the overlay before the IME/virtual input is used; ranges and ordinary
   toggles can remain open. The input hold from #376 stays in force until
   Resume succeeds. Never call a hide-only method that leaves korrid's overlay
   intent or stream input hold behind.

`E:22–46,228–247,336–375` already supplies a closed effect vocabulary,
validation and typed current values. Use those operations as extraction
sources. Its coordinator has an old executor publication design, but Game
currently only implements `Actions`; it does not register that coordinator.
Do not revive its executor registration/publication protocol or write a
parallel state store. The exact `V.Session` lifecycle is the live owner.

## Exact proposed surface treaty delta

**Controls need no new Rust type, interaction variant or request.**
`T:682–698,704–725,769–773` covers commands, boolean toggles, string choices
and numeric ranges. Append the viewer group to `presentation.groups`.
Change the “plugin-owned” doc comment (`T:718`) to cover viewer groups too;
there is no source enum, Android key, effect name or touch flag in the treaty.
The viewer filters by actual support before publication.

Time and game name reuse `SurfaceModel.clock_label` and
`SurfaceGameplayOverlayPresentation.title`. Merge the root's clock into the
overlay model in `SurfaceHub::model`; do not retain the `None` placeholder.

Proposed insertions in `SurfaceGameplayOverlayPresentation` (`T:731–743`):

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub battery_percent: Option<f64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub stream_health: Option<SurfaceStreamHealth>,
```

Proposed new type, with the same derives/Typeshare convention as its neighbors:

```rust
#[typeshare]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStreamHealth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_bitrate_kbps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames_dropped: Option<f64>,
}
```

| Addition | Grounding and precise projection |
|---|---|
| `battery_percent` | Android capacity property above; legacy `DeviceBatteryReady.percent` and normalization at `device-facts.ts:16–23,252–255`. Nullable numeric percent becomes `Option<f64>`, finite whole 0–100; no zero/default for absent/invalid reads. |
| `stream_health` | Groups the three facts #378 asks for, from the existing legacy `StreamHealthSample`, not a new health scoring scheme. Absent with no valid sample/viewer; all-absent projections are omitted. |
| `rtt_ms` | Legacy `stream-health.ts:4` `rttMs?: number`; current packed native RTT above. Finite nonnegative whole ms in the uint32 producer range; sentinel `-1` never crosses the boundary. |
| `delivered_bitrate_kbps` | Legacy `stream-health.ts:7` and patch `:231–232`; finite nonnegative video kbps, measured per interval. No product maximum invented from the configured 500–150000 kbps setting. |
| `frames_dropped` | Legacy `stream-health.ts:11` and patch `:227,350–384`; finite nonnegative whole count for the current sample interval. Not `VideoStats.framesLost`, not cumulative frames, not percent. |

This is a **presentation projection**, not a port/redesign of legacy's general
device or telemetry schema. Specific deltas from that baseline: the strip has
no consumer for charging/status/supplies/error records, so only its nullable
percent reaches the treaty; it has no consumer for telemetry sequencing,
timestamps, variance, packet loss, FPS, decode time, queue depth or first-frame
time, so those legacy fields do not cross it. Preserve any internal sample
identity/freshness needed by the producer; do not invent replacements in the
surface treaty. Cost: a surface cannot draw a history, a charging indicator or
other diagnostics from this projection. Those need a grounded follow-up.

No generated file changes in Phase 1. Implementation regenerates
`contracts/generated/` with `nix run .#korrid-check`.

## Pico and Zone: model obligations, not visual design

- Only `GameplayOverlay` draws this strip, from clock, battery, health and
  title. It never consumes input or appears in a focus graph. Missing facts
  are visibly unavailable, never guessed. The active package owns wording,
  units and every pixel; no Java TextView or WebView draws the Korri strip.
- Both packages render the extra labelled group with their existing gameplay
  control renderer and focus behavior. They do not inspect viewer identity,
  Android keys or hardware to decide which rows exist. Existing plugin groups
  and Resume/End/Disconnect/Open Korri remain separate.
- Their existing adapters (`pico/src/pico-overlay-view.ts:104–121`,
  `pico/src/native/sheet.ts:109–141`; Zone's `logic.ts:313–316` overlay path)
  need to pass read-only strip facts to their views. No layout, colors, sizes
  or new interaction design is prescribed here.

## Implementation proof and overlap

Phase 1 verifies source extraction only; it does not claim either issue done.
Implementation must test snapshot validation, capability omission on no-touch
viewers, stale launch/generation/lease rejection, coupled value readback,
unavailable samples, no preference writes, recreation of the same stream,
and reset to baseline on a new stream. Exercise every included operation,
not only a changed label. In particular verify the PiP effect and IME/input
hold ordering on the phone.

The final real-path check is phone → aka streaming: open the overlay in Pico
and Zone; observe time, phone battery, live RTT/video bitrate/drop count;
close and prove the strip disappears. Change controls and prove the stream
changes, then start a new stream and prove baseline values return. Check
that the game and audio keep running and no overlay input reaches the game.

**#394 overlap:** current main already has
`docs/adr/0055-a-tap-activates-the-target-the-surface-drew.md` (renumbered from
0054). It explicitly prohibits concurrent implementation with #378/#379.
Overlap is `contracts/surface/src/lib.rs`, generated `surface.ts`, both
packages' overlay logic/views/focus tests, and Android Surface input hosting.
This note changes none of those. Whichever implementation starts second must
rebase; reuse #394's activation/range behavior rather than add pointer events,
callbacks or focus ownership to this path. Floating-button/overlay entry
remains a separate gap, not something the model silently solves.
