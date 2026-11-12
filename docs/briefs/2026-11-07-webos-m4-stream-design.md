# webOS M4: installed Moonlight stream edge — research and decision gate

**Status: research complete; implementation and IPK blocked on the exit contract.**
No TV or haku connection was made. No other app's certificate, private key or
state was read. This is not an M4 playback proof. The supervisor explicitly
stopped implementation at this decision gate and ruled out packaging an
unchanged binary.

The chosen viewer remains the installed `com.limelight.webos`, Moonlight TV
1.6.36, through AppManager. Do not embed Moonlight, change Zone, add a runner to
the TV, import another app's state, or invent TV configuration to bridge gaps.

## Sources and confidence

| Source inspected | Pin | What it proves |
| --- | --- | --- |
| Korri | `09aa6bb21abec648ffd0ad15a048a0f675e5363b` | Existing selection, viewer and certificate contracts. |
| [moonlight-tv v1.6.36][ml] | `89221f66b79e06178a6465503cc5a306d2794786` | The selected application's source behavior. |
| [SDL-webOS release-2.30.12-webos.5][sdl] | `9f30a1f01e4d36aa5d3e4e04df5921b7a9d093ee` | Moonlight's pinned backport's relaunch handling. Moonlight pins it in [CMakeLists.txt:135–138][ml-sdl]. This is **not** a proposal to bundle it in Korri. |
| [Korri plugins][plugins] | `57de061520cc010cbc5ebe9b978f4799d159affa` | Reviewed Sunshine producer/configuration. Not evidence of haku's installed version. `services/sunshine/` is in this repository, not Korri core. |
| [Sunshine v2025.924.154138][sun] | `86188d47a7463b0f73b35de18a628353adeaa20e` | The base version selected by [approved-patches.nix:1–3][plugin-base]. The inspected Korri patches do not replace its app-ID calculation. |
| [webOS OSE SAM][sam] | `96161d3e50670fb21daccfb12a75c35fe406ce0e` | Public AppManager implementation reference, **not** the B7's proprietary webOS 3.9 implementation or its jail permissions. |
| [LG TV Application Manager reference][tv-api] | Retrieved during this workstation review | Public TV `launch` contract; not a promise that private lifecycle APIs work on the B7. |

**Verified** below means inspected pinned source or a workstation calculation.
**Inferred** means a proposed composition of those contracts. TV permissions,
actual haku configuration, ARM execution and foreground behavior remain untested.
The existing [webOS README](../../clients/shell/webos/README.md) and
[LG input/power report](2026-10-30-lg-b7-native-input-report.md) still apply.
Do not change SDL, screensaver registration or power policy to make this work.

## 1. What Moonlight actually accepts and reports

### Initial launch can select a known host and an application

Verified at v1.6.36:

- [app_webos.c:39–82][ml-params] parses JSON **only from initial `argv[1]`**.
  Its recognized fields are `host_uuid` (string) and `host_app_id` (32-bit
  integer, also accepted as a decimal string). There is no host-address,
  app-name, Korri launch-ID, key-directory, return-app or exit-on-stream-end
  parameter in that parser.
- [main.c:29–45][ml-main] calls that parser once, opens Moonlight's launcher,
  then runs the application loop. This is a launch-parameter interface, not a
  general URI deep-link router or a new Luna service offered by Moonlight.
- [launcher.controller.c:502–515][ml-host-select] selects the matching UUID
  from Moonlight's known host nodes. [Lines 280–289][ml-app-select] pass the
  requested app ID once. [apps.controller.c:507–518][ml-auto-start] starts the
  matching item after the host's app list loads. It does not resolve an app by
  name. Missing/unpaired/unreachable hosts or absent app IDs do not establish
  a stream; [launcher.controller.c:101–114][ml-pair-gate] opens pairing when
  appropriate instead.
- **A warm launch is not equivalent to a cold launch.** Moonlight's pinned
  SDL [registers for native app lifecycle and handles `relaunch` by raising
  the existing window][sdl-relaunch]. It does not pass new launch parameters
  to Moonlight's startup parser. Sending a second `launch` with another app ID
  to an already-running Moonlight cannot be treated as starting that stream.
  The edge must not close a pre-existing user-owned Moonlight instance just
  to obtain a cold start.

### Known hosts and client identity are owned by Moonlight

| Item | Existing producer and location |
| --- | --- |
| Configuration root | [path_webos.c:10–21][ml-path] uses `$HOME/conf`; on directory-creation failure it uses `/tmp/moonlight-tv/conf` and marks preferences nonpersistent. Actual HOME and persistence in Moonlight's jail were not inspected. |
| Known hosts | [known_hosts.c:30–67,71–145][ml-hosts] reads/writes `hosts.ini` below the configuration root. Each INI section is the Sunshine UUID; values include `mac`, `hostname`, `address`, selection and favorites/hidden app IDs. This is a **known-host list**, not a saved client-private-key or a local paired boolean. |
| Client identity directory | [app_settings.c:75–78][ml-keydir] selects `conf/key`. [backend_gs.c:16–27][ml-key-init] loads or initializes that identity through libgamestream. |
| Public certificate / private key | [http.h:24–25][ml-key-names] names `client.pem` / `key.pem`; [conf.c:124–160][ml-key-load] loads both from the key directory. Thus normally `$HOME/conf/key/client.pem` and `$HOME/conf/key/key.pem`. `uniqueid.dat` is a separate client identifier ([priv.h:7][ml-client-id], [conf.c:90–104][ml-id-load]). |
| Pairing state | [client.c:719–783][ml-status] attempts authenticated HTTPS serverinfo, falls back to HTTP, and reads `PairStatus`. Sunshine's accepted client certificate, not a new Korri TV record, supplies the trust grant. |

Do not assume Korri's own `/media/developer/org.korri.shell` allocation applies
to Moonlight. Do not read or copy its `key.pem`. The stored host UUID is the
same `uniqueid` read from GameStream serverinfo ([client.c:767–768][ml-status]),
not haku's Korri device public key.

### Stream completion does not exit Moonlight

Verified:

- [session_worker.c:95–127][ml-end-worker] waits for interruption, posts
  `USER_STREAM_CLOSE`, stops the connection, optionally sends GameStream quit,
  then posts `USER_STREAM_FINISHED`. These are **in-process SDL/bus events**.
- [ui/root.c:175–200][ml-end-ui] reopens **Moonlight's own launcher** on close.
  [streaming.controller.c:166–179][ml-end-overlay] removes the streaming UI on
  finished. Neither action exits the application or notifies Korri via Luna.
- Moonlight's overlay distinguishes **Disconnect** from **Quit game**
  ([streaming.view.c:97–111][ml-quit-labels]). The handlers pass `quitapp=false`
  and `true`, respectively ([streaming.controller.c:258–265][ml-quit-actions]).
- Application exit is separate: [app.c:285–286][ml-exit] clears `running`;
  [main.c:38–45][ml-main] later exits with the initialization return code.
  There is no per-stream result conveyed as a process exit status.

**High-importance inherited security gap:** the pinned installed client's
[http.c:69–75][ml-tls] disables cURL hostname and peer-certificate verification.
PIN pairing authenticates the pairing exchange, but do not describe subsequent
cURL requests as certificate-pinned TLS. Korri's Linux adapter obtains a server
certificate; this Moonlight launch interface has no parameter to consume it.
No TLS bypass or key access was added here. Choosing the installed app inherits
its existing behavior; fixing that app is separate work and changes the pin.

## 2. Existing korrid seam and the return path

The seam already exists. No surface treaty change is needed to pick a real
peer game or to declare a viewer:

1. [upstream_native.rs:301–319](../../services/korrid/src/upstream_native.rs)
   prepares the selected game through native signed RPC. Its nearby
   `stream_play` method (322–344) registers a player in an **already-running**
   exact session; it is not the first-game launch endpoint.
2. [upstreams.rs:1018–1059](../../services/korrid/src/upstreams.rs) prepares
   the selected peer route, receives the real launch ID, and calls
   `viewer.start(launch_id, moonlight_address)`. On viewer-start failure it
   compensates by stopping that exact streamed launch (1064–1086).
3. [LinuxStreamViewer:33–56](../../services/korrid/src/linux_viewer.rs) owns
   start, status, background, resume/focus and stop by **exact launch ID**.
   It forbids selecting a viewer merely by process name/address or latest state.
   Keep the existing seam despite its Linux name; do not make Zone launch Luna.
4. [EmbeddedPlatform:5386–5394 and startup:5497–5501](../../services/korrid/src/lib.rs)
   injects that viewer through `with_linux_viewer`. The field is crate-private;
   wiring the separate webOS binary needs a narrow public composition entry to
   it, not another viewer registry. [webos-korrid.rs:159–169](../../services/korrid/src/bin/webos-korrid.rs)
   currently supplies only product relays and no viewer.
5. The Linux implementation bootstraps its own Moonlight identity, reads
   serverinfo, provisions its certificate and launches the immutable plugin
   through exact systemd/focus authorities
   ([production.rs:554–657](../../services/korrid/src/linux_viewer/production.rs)).
   Its app selector is the name `Korri Stream` (345–354), not a numeric ID.

### The streamed-session watch solves one direction, not both

[Embedded startup:5526–5531,5595–5604](../../services/korrid/src/lib.rs)
already starts `watched_upstream` whenever a viewer is supplied.
[spawn_watch:80–91](../../services/korrid/src/stream_play/device.rs) polls the
selected peer once per second, including while the shell is behind the viewer.
When the **streamed game ends**, [upstreams.rs:1249–1282](../../services/korrid/src/upstreams.rs)
stops the exact viewer and clears selection. The webOS edge could implement
that stop with a confirmed close of its owned Moonlight instance, followed by
a confirmed return of the existing shell to the foreground. It need not wait
for Moonlight to exit itself.

But this does **not** detect the user disconnecting in Moonlight:

| Event | Existing observation and consequence |
| --- | --- |
| Actual haku game exits / exact Korri session ends | Native session poll can drive viewer stop and return to Zone. This part is grounded. |
| Moonlight Disconnect | Moonlight returns to its launcher; its process and the streamed Korri game remain live. No stream-ended signal reaches the TV's korrid. |
| Moonlight Quit game | Moonlight sends GameStream quit. Sunshine [nvhttp.cpp:1004–1026][sun-cancel] terminates its streams and its own selected app. The reviewed app has an empty command, so it is a capture placeholder, not the game process korrid owns. No native Korri session-end notification is implemented by that handler. Do not equate it with `app.session.end`. |
| Moonlight process closes / shell is manually foregrounded while streamed game remains live | [recover_viewer:1099–1127](../../services/korrid/src/upstreams.rs) restarts a stopped viewer or refocuses a backgrounded one while the holder phase still requests running. Merely reporting `Stopped` or `Backgrounded` would restart/refocus Moonlight, not perform a deliberate quit. |

Therefore the existing watch supports **streamed-game-ended → return**, not the full
**Moonlight-stream-ended → return** requirement. A new UI meaning for quit,
a real stream-lifecycle producer, or a changed Moonlight is still a decision.
Do not invent a stream-end timeout, parse another app's logs, or turn Sunshine's
public global `currentgame` value into exact Korri session authority.

## 3. Sunshine UUID and numeric app ID

### Host UUID requires no saved webOS configuration

The Linux edge already performs bounded HTTP `/serverinfo` on the selected
`moonlightAddress`: [production.rs:126–145,574–605](../../services/korrid/src/linux_viewer/production.rs).
It uses a five-second client deadline, no redirects, a 64 KiB body limit and
validates the returned `uniqueid`. Sunshine's [serverinfo:694–701][sun-serverinfo]
emits that UUID over its existing HTTP interface. The operator-reported
`192.168.1.140:47989` is a prior reachability observation, **not** a new default
address to save in the TV edge.

Korrid's existing authenticated certificate broker
[upstreams.rs:876–917,920–942](../../services/korrid/src/upstreams.rs) attests
which native peer owns an expected UUID, and refuses ambiguity. This provides
a grounded way to check a UUID read over unauthenticated HTTP against signed
peer authority. The read alone is not authentication. No UUID field or file
needs to be added to TV configuration.

### The reviewed Korri Stream ID is deterministic

The plugins repository's [plugins/sunshine/apps.json:1–4][plugin-apps] contains
one app, `Korri Stream`, with empty `cmd` and `image-path`.
[plugin.nix:14–29][plugin-app-package] packages those exact native bytes;
[runtime.sh:50–51][plugin-runtime] selects them with Sunshine's `file_apps`.
This is the source of truth, not a proposed new app manifest.

Sunshine [process.cpp:458–461][sun-image] maps an empty image path to its default
image. [Lines 529–563][sun-id] then compute CRC-32 of the name alone and take
`abs((int32_t)crc)`. [Lines 669–677][sun-id-select] use that result unless an
already-seen ID collides, in which case they include the app index. For the
reviewed single-app configuration there is no earlier collision:

```text
CRC32(UTF-8 "Korri Stream") = 0x937f911c
signed 32-bit value        = -1820356324
Sunshine app ID            = 1820356324
```

Verified by compiling Sunshine's unmodified `calculate_crc32` function from
that pin with Boost and applying its signed conversion. A workstation zlib
calculation and independent reflected-polynomial (`0xedb88320`) calculation
agree. This is **not yet a read of haku's actual `/applist`**. An image/name/configuration or Sunshine algorithm change
can change the ID. A new ID constant must cite this producer and be checked
against the deployed host; no per-TV setting or persisted mapping is justified.

The authoritative runtime list is Sunshine's authenticated HTTPS
[applist:782–806][sun-applist], containing `AppTitle` and `ID`. It is not served
by unauthenticated HTTP. Current Korri `SessionPrepared`
([lib.rs:180–184](../../services/korrid/src/lib.rs)), `SelectedGameLaunch`
([game_routes.rs:68–71](../../services/korrid/src/game_routes.rs)) and
`ActiveSession` ([lib.rs:615–637](../../services/korrid/src/lib.rs)) do **not**
carry Sunshine UUID/app ID. Adding such metadata is a native wire change, not
an existing field to consume. Computing the ID from the reviewed producer and
reading/attesting serverinfo avoids that change for this real case, at the cost
of depending on the exact host app configuration.

## 4. AppManager lifecycle and foregrounding

[LG's public TV reference][tv-api] documents `launch` with `id` and `params`,
including bringing up one's own app from a background service. Its result is
request success/failure. It is **not** an Activity-style result callback when
the other app closes. Its `getAppLoadStatus` checks installation, not lifetime,
and is documented only for TV 4.0 and later; do not require it on this B7.

OSE SAM supplies useful contracts to test, but none is yet verified in the
`org.korri.shell` jail:

| API / behavior | Pinned OSE source and limit |
| --- | --- |
| `getAppLifeEvents` | [ApplicationManager.cpp:326–341,678–757][sam-life] requires a subscription. Events include `launch`, `foreground`, `background`, `close` and `stop`, with `appId` and `instanceId`. `close` is closing; `stop` is removal. These are app events, not stream events. |
| `running` | [ApplicationManager.cpp:308–323,1021–1025][sam-running] supports a snapshot and optional subscription. [RunningApp.h:282–307][sam-running-fields] uses `id`, **`processid`** (string), `webprocessid`, `instanceId` and `launchPointId` in running entries. Do not assume `processId` or app-event field casing here. |
| Native process termination | [NativeContainer.cpp:29–53][sam-child] handles the child exit, removes that exact process, and [RunningAppList.cpp:377–381][sam-remove] sets `STOP` and posts the new running list. Numeric wait status is logged internally; the launch caller does not receive a stream exit code. |
| Foreground/relaunch | [ApplicationManager.cpp:261–276][sam-launch] relaunches an existing app. [PolicyManager.cpp:105–131][sam-relaunch] sends a lifecycle event to a registered native app; an unregistered native app is closed and launched again. This makes shell PID/brain continuity a necessary TV check, not an assumption. |
| Closing an app | [ApplicationManager.cpp:291–305][sam-close] starts close; [NativeContainer.cpp:235–258][sam-native-close] requests termination. The documented OSE `closeByAppId` [schema:1–12][sam-close-schema] targets `id`, not a Korri launch ID. Check the B7's actual supported method and exact-instance targeting before implementing stop. |

A candidate return call is the **existing** launch contract with
`id: org.korri.shell`. It must bring the *same* shell and embedded brain back;
a restart loses the in-memory selected-session authority. The launcher
[launch.sh:10–15,38–49,81–88](../../clients/shell/webos/launch.sh) ignores launch
JSON and tears down the brain when the shell ends. No registered relaunch
adapter was added here. Do not fix this by bundling Moonlight's SDL or entering
the TV power-service path rejected in the input report.

## 5. Decisions to take before implementation

These are recommendations, **not recorded product choices**. Ask the owner one
question at a time, starting with what quit must mean. No new schema is proposed.

| Decision | Grounded options, cost and recommendation | Treaty / wire impact |
| --- | --- | --- |
| What must quit observe? | **Original requirement:** Moonlight Disconnect/stream failure must return to Zone. Installed v1.6.36 exports no such signal. **Narrower option:** return only when the streamed Korri game ends; its existing poll is sufficient, but Disconnect stays in Moonlight and the full M4 check is not met. **Another option:** expose an exact stream-end producer from Sunshine or change Moonlight to exit/return on end; this adds cross-deployable work and may change the chosen pin. **Recommendation:** do not relabel the narrower route as M4 complete. Keep the full exit requirement blocked until the owner chooses a signal/exit route. | Streamed-session-end handling needs no new wire. New Sunshine stream lifecycle metadata would need a producer and native treaty review. A changed Moonlight launch/exit contract changes that external interface; it need not change SurfaceModel. No fields are invented here. |
| Pairing | **One-time Moonlight PIN:** [pair.dialog.c:60–79][ml-pin-ui] shows the PIN; [worker/pairing.c:16–29][ml-pin-worker] runs `gs_pair`. No private-key access by Korri, but user action and access to haku's existing Sunshine PIN submission path are required. The plugin does not advertise Sunshine's admin-web port, so do not assume LAN access to it. **Existing korrid provisioning:** submit this installed client's **public** `client.pem` using `app.moonlight.certificate.provision`; the broker/adapter already exists, but there is no approved cross-app certificate export or TV CLI flow. Public-cert export does not require copying `key.pem`. **Separate new korrid identity:** not usable by installed Moonlight; launch params do not let it select that key directory. **Recommendation:** use the app's PIN flow once if the existing host PIN path is available. Otherwise ask about a public-certificate export/provision operation, not private-key sharing. | PIN uses existing protocols; no Korri schema/wire change. Provision request already has `hostUuid`, `clientCertificate`; reply has `serverCertificate` ([lib.rs:229–249](../../services/korrid/src/lib.rs)). Its local-control listener is explicitly unsupported (3155–3187); do not claim a shipped `korri` provisioning CLI. Any cross-app export/configuration remains unresolved, not an invented file path. |
| Numeric app ID source | **Reviewed producer calculation:** `1820356324`, after checking haku runs that app configuration; simple and no secret, but tied to the name/default image and version. **Runtime `/applist`:** authoritative, but requires an authorized GameStream client identity; do not read Moonlight's private key to query it. **New native metadata:** no client key transfer, but adds a host producer/wire. **Recommendation:** first verify the reviewed deterministic value against haku's actual installed app list; use that grounded case without a TV mapping schema. | Calculation needs no wire change. Publishing app ID from korrid would be a native wire change. |
| Existing app ownership and return | **Refuse warm Moonlight:** no loss of another user's stream; requires the user to quit it before first play. **Automatically close/relaunch it:** could end an unrelated stream; not authorized by the chosen viewer contract. **Recommendation:** refuse pre-existing Moonlight and bind an observed new process/instance to the Korri launch. Prove close targeting and return-to-same-shell on the B7 before wiring. App-ID-only observation is not exact-instance authority. | An in-memory process/launch association is edge state, not a new persisted schema. B7 lifecycle/close API evidence is missing. New persistent process records are not justified. |

Candidate design once those decisions and TV contracts are resolved:

- Supply a webOS implementation at the existing viewer seam from the embedded
  composition. It shells out to `luna-send`; no platform effects go into Zone,
  federation logic or a plugin declaration.
- Resolve the selected Moonlight address through the existing route; read and
  attest the real Sunshine UUID. Never hardcode haku's IP or key as product
  configuration. Keep Moonlight's known hosts and identity in Moonlight.
- Refuse a pre-existing instance; cold-launch through AppManager with its
  existing `host_uuid` and `host_app_id` fields. Bind only the observed newly
  launched instance to the exact Korri launch. Do not claim a successful Luna
  request proves first-frame playback.
- Reuse the existing streamed-session watcher for holder-side cleanup. Stop only the
  owned exact instance and confirm the original shell is foregrounded. Keep
  errors explicit so the core's existing compensation path remains authoritative.
- Do not implement the separate local stream-end direction until its real
  producer is selected. Do not fabricate `Stopped` or session completion.
- Add bounded subprocess output/deadlines and deterministic tests using an
  executable `luna-send` fixture, without global PATH races: cold launch,
  rejection, unavailable service, malformed/oversize output, timeout, mismatched
  identity, close-versus-stop, foreground confirmation, repeated stop, and no
  mutation of an unrelated instance. Add an embedded real-RPC regression that
  streamed-session end invokes the viewer and does not restart it.

No implementation is common to all unresolved exit/ownership routes beyond
what already exists. Extracting helpers or exporting a new platform API now
would be speculative work, so this change is documentation only.

## 6. Tomorrow: bounded operator verification, not tonight's work

Do not install anything from this documentation change. There is no new IPK.
Keep `org.korri.slint-spike` unchanged. Perform all Luna checks as the normal
`org.korri.shell` app UID **inside its jail**, not by bypassing denied methods
as root. Record actual error payloads and stop on a denied/unsupported method.
Do not print capabilities, private keys or entire private-state files.

### First collect the missing platform evidence

1. Turn the TV on and confirm its UI responds. Record installed Moonlight
   version and haku's actual Sunshine version/native `file_apps` record without
   modifying either. Check the expected `Korri Stream` ID through an already
   authorized `/applist` consumer or existing admin UI. If it differs from
   `1820356324`, stop; do not add a fallback setting.
2. In the shell jail, collect lifecycle snapshots:

   ```sh
   luna-send -n 1 luna://com.webos.applicationManager/running '{}'
   luna-send -n 1 luna://com.webos.applicationManager/getForegroundAppInfo '{}'
   ```

   Record actual B7 field names and instance/process identity. In a separate
   terminal, if the service permits subscriptions, run the following until
   the operator presses Ctrl-C (no unattended timer):

   ```sh
   luna-send -i luna://com.webos.applicationManager/getAppLifeEvents '{"subscribe":true}'
   ```

   A successful subscription acknowledgement is not a close event. Observe
   `close` versus actual process removal/`stop` and a new `running` snapshot.
3. Only after choosing the pairing option, pair Moonlight with haku using its
   own UI. For PIN pairing, have the user ready before starting the pairing
   dialog's timed exchange. Add haku's observed address in Moonlight if absent.
   Confirm it lists `Korri Stream`. Do not export `key.pem` or weaken firewall,
   TLS, signature or certificate permissions.
4. Quit Moonlight **the application**, not just its stream, using its own UI.
   Confirm it is absent from `running`. Obtain haku's public serverinfo UUID
   using the existing selected address and attest it through the existing
   native broker. Substitute that observed UUID below; do not use a Korri
   device key. After verifying the numeric ID in step 1, cold-launch:

   ```sh
   luna-send -n 1 luna://com.webos.applicationManager/launch \
     "{\"id\":\"com.limelight.webos\",\"params\":{\"host_uuid\":\"$HOST_UUID\",\"host_app_id\":1820356324}}"
   ```

   With no new korrid viewer, this only tests the external app's selector;
   it does not prove that selecting a Zone game launches haku's real game.
5. Check video, sound and physical controller input. Exercise **Disconnect**,
   then in a separate cold-start trial **Quit game**. Record which Moonlight
   screen appears, whether its process/instance remains, and whether haku's
   **Korri** session is still active. Do not infer game end from Sunshine's
   global `currentgame` alone. Manually end the exact test game through its
   normal control before another launch; never stop a different launch.
6. While Moonlight runs, record the shell and embedded-brain process identities,
   then test the candidate return operation:

   ```sh
   luna-send -n 1 luna://com.webos.applicationManager/launch '{"id":"org.korri.shell"}'
   ```

   Confirm Zone is visible and both original process identities remain. If
   launch restarts the shell/brain, stop: this is not a valid background/focus
   adapter. Do not retry with a new SDL or TV-power registration.
7. Determine which B7 close method can target that exact observed instance and
   whether the shell jail may use it. OSE's app-ID-only `closeByAppId` is not
   proof of exact targeting. Do not test close against a pre-existing or
   replaced instance. Record before deciding the implementation.

### After the decisions, implementation and tests

The actual M4 acceptance must select a real haku game in Zone, show that exact
game with sound and working controls, and exercise the **chosen** quit path
back to the same Zone process. Test streamed-game-end and viewer-disconnect as
separate cases. Check a second game launch, launch failure compensation and
that no haku game remains unintentionally live. A manual Moonlight selector
trial is not this acceptance.

Only then build/package a clean committed candidate on the workstation, one
expensive command at a time, using two Cargo jobs:

```sh
export CARGO_BUILD_JOBS=2
source_sha=$(git rev-parse HEAD)
test "$(git rev-parse HEAD)" = "$source_sha"
nix run .#shell-webos-build
# Run focused viewer + embedded tests, typecheck and linter before packaging.
# Choose a fresh output directory and set OUT to it.
test "$(git rev-parse HEAD)" = "$source_sha"
nix run .#shell-webos-package -- "$OUT" --source-sha "$source_sha"
nix run .#shell-webos-package-check -- --check-ipk "$OUT/org.korri.shell_0.0.0_arm.ipk"
sha256sum "$OUT/org.korri.shell_0.0.0_arm.ipk"
```

Record the source SHA, IPK path and SHA-256 from that build. Uninstall only
`org.korri.shell` before reinstalling, as its README requires; preserve its
allocated persistent state and the separate installed Moonlight app. No target
builds. No such artifact was made for this documentation-only decision gate.

## Workstation checks for this research change

- Compiled and ran the pinned Sunshine CRC function with `-Wall -Wextra
  -Werror`: `937f911c 1820356324`. Independent zlib/bitwise check passed.
- Checked all 46 pinned source file/line-range citations against the checked-out
  commits, all 19 local links, reference labels and shell blocks (`bash -n`).
- `git diff --check` passed. No repository-wide Markdown formatter is declared.
- No production code changed, so no Rust build, typecheck, clippy, test suite,
  ARM build or package command was run. No simulated Luna test is presented as
  proof of the unselected exit route. Real playback remains untested.

[ml]: https://github.com/mariotaku/moonlight-tv/tree/89221f66b79e06178a6465503cc5a306d2794786
[ml-params]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/platform/webos/app_webos.c#L39-L82
[ml-main]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/main.c#L29-L45
[ml-host-select]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/launcher/launcher.controller.c#L502-L515
[ml-app-select]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/launcher/launcher.controller.c#L280-L289
[ml-auto-start]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/launcher/apps.controller.c#L507-L518
[ml-pair-gate]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/launcher/launcher.controller.c#L101-L114
[ml-path]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/platform/webos/path_webos.c#L10-L21
[ml-hosts]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/backend/pcmanager/known_hosts.c#L30-L145
[ml-keydir]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/app_settings.c#L75-L78
[ml-key-init]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/backend/backend_gs.c#L16-L27
[ml-key-names]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/core/libgamestream/libgamestream/http.h#L24-L25
[ml-key-load]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/core/libgamestream/src/conf.c#L124-L160
[ml-client-id]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/core/libgamestream/src/priv.h#L7
[ml-id-load]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/core/libgamestream/src/conf.c#L90-L104
[ml-status]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/core/libgamestream/src/client.c#L719-L783
[ml-end-worker]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/stream/session_worker.c#L95-L127
[ml-end-ui]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/root.c#L175-L200
[ml-end-overlay]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/streaming/streaming.controller.c#L166-L179
[ml-quit-labels]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/streaming/streaming.view.c#L97-L111
[ml-quit-actions]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/streaming/streaming.controller.c#L258-L265
[ml-exit]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/app.c#L285-L286
[ml-tls]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/core/libgamestream/src/http.c#L69-L75
[ml-pin-ui]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/ui/launcher/pair.dialog.c#L60-L79
[ml-pin-worker]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/backend/pcmanager/worker/pairing.c#L16-L29
[ml-sdl]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/CMakeLists.txt#L135-L138
[sdl]: https://github.com/webosbrew/SDL-webos/tree/9f30a1f01e4d36aa5d3e4e04df5921b7a9d093ee
[sdl-relaunch]: https://github.com/webosbrew/SDL-webos/blob/9f30a1f01e4d36aa5d3e4e04df5921b7a9d093ee/src/core/webos/SDL_webos_init.c#L169-L239
[plugins]: https://github.com/korri-os/plugins/tree/57de061520cc010cbc5ebe9b978f4799d159affa
[plugin-base]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/services/sunshine/approved-patches.nix#L1-L3
[plugin-apps]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/sunshine/apps.json#L1-L4
[plugin-app-package]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/sunshine/plugin.nix#L14-L29
[plugin-runtime]: https://github.com/korri-os/plugins/blob/57de061520cc010cbc5ebe9b978f4799d159affa/plugins/sunshine/runtime.sh#L50-L51
[sun]: https://github.com/LizardByte/Sunshine/tree/86188d47a7463b0f73b35de18a628353adeaa20e
[sun-image]: https://github.com/LizardByte/Sunshine/blob/86188d47a7463b0f73b35de18a628353adeaa20e/src/process.cpp#L458-L461
[sun-id]: https://github.com/LizardByte/Sunshine/blob/86188d47a7463b0f73b35de18a628353adeaa20e/src/process.cpp#L529-L563
[sun-id-select]: https://github.com/LizardByte/Sunshine/blob/86188d47a7463b0f73b35de18a628353adeaa20e/src/process.cpp#L669-L677
[sun-applist]: https://github.com/LizardByte/Sunshine/blob/86188d47a7463b0f73b35de18a628353adeaa20e/src/nvhttp.cpp#L782-L806
[sun-serverinfo]: https://github.com/LizardByte/Sunshine/blob/86188d47a7463b0f73b35de18a628353adeaa20e/src/nvhttp.cpp#L694-L701
[sun-cancel]: https://github.com/LizardByte/Sunshine/blob/86188d47a7463b0f73b35de18a628353adeaa20e/src/nvhttp.cpp#L1004-L1026
[tv-api]: https://webostv.developer.lge.com/develop/references/application-manager#launch
[sam]: https://github.com/webosose/sam/tree/96161d3e50670fb21daccfb12a75c35fe406ce0e
[sam-life]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/src/bus/service/ApplicationManager.cpp#L326-L757
[sam-running]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/src/bus/service/ApplicationManager.cpp#L308-L323
[sam-running-fields]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/src/base/RunningApp.h#L282-L307
[sam-child]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/src/bus/client/NativeContainer.cpp#L29-L53
[sam-remove]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/src/base/RunningAppList.cpp#L377-L381
[sam-launch]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/src/bus/service/ApplicationManager.cpp#L261-L276
[sam-relaunch]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/src/manager/PolicyManager.cpp#L105-L131
[sam-close]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/src/bus/service/ApplicationManager.cpp#L291-L305
[sam-native-close]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/src/bus/client/NativeContainer.cpp#L235-L258
[sam-close-schema]: https://github.com/webosose/sam/blob/96161d3e50670fb21daccfb12a75c35fe406ce0e/files/schema/applicationManager.closeByAppId.schema#L1-L12
