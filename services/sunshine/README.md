# sunshine-korri

`sunshine-korri` is Korri's downstream Sunshine package for carried patches that are useful to Korri before they are upstreamed, redesigned, or retired.

The package is intentionally an umbrella, not a single-feature fork. Patches may be unrelated. Each patch should explain:

- what behavior it changes,
- whether it is experimental or intended for upstreaming,
- what evidence exists,
- when it can be removed.

## Current patches

### V4L2 M2M encoder profile

Patch `0021-add-v4l2m2m-encoder.patch` exposes Sunshine encoder name `v4l2m2m`. It maps H.264 to FFmpeg `h264_v4l2m2m` and HEVC to `hevc_v4l2m2m`. The backend accepts host-memory NV12 frames and relies on the Linux V4L2 M2M codec API. The Linux-host module selects it with `services.korriLinuxHost.sunshine.encoder = "v4l2m2m"`, selects the approved aarch64 package profile, and sets `SUNSHINE_STRICT_ENCODER=1` so an unavailable device or codec rejects the stream instead of falling back to software.

The `sunshine-korri-v4l2m2m` package uses the same reviewed Sunshine source and Korri patch set as `sunshine-korri`. It replaces only Sunshine's prepared FFmpeg libraries. Those libraries come from historical `build-deps` commit `2851db101eeddae8f02489d48a52a4d83e6f7e7b`, which introduced the reviewed FFmpeg commit `61c50407fd429a5e2ec616e2e846c3fe3743879a`. Korri applies the buffer-layout fix from FFmpeg PR `#24328`. The fix copies visible rows into the driver-aligned storage width and height. This prevents the `ff_v4l2_buffer_avframe_to_buf()` crash seen with the `qcom-iris` encoder at standard `1280x720` and `1920x1080` frame sizes.

The Odin 2 Portal kernel carries upstream Linux commit `6f62dcefd2494aa9ac01538372353bf07755491e`, `media: qcom: iris: Add request key frame support for encoder`. It maps `V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME` to the Iris encoder's request-sync-frame property. Sunshine needs this control for Moonlight recovery requests and stream refreshes after loss.

Rockchip RKMPP is a separate encoder profile. The RG353M implementation uses `h264_rkmpp` and `/dev/mpp_service`; it does not use V4L2 M2M. The standard `sunshine-korri` aarch64 software profile remains unchanged so this Portal profile does not replace or pre-empt the RG353M path.

A second, Korri-local FFmpeg patch adds `repeat_headers=1`. Sunshine sets this option to join the sequence header to the first frame and repeat parameter sets at every IDR. Encoder creation fails if the requested header controls do not read back correctly. The default FFmpeg behavior is unchanged when the option is absent. Remove this patch when the reviewed FFmpeg version supplies equivalent in-band header controls.

A second, Korri-local Iris patch subscribes to `HFI_PROP_PICTURE_TYPE` on encoder bitstream output. Linux 7.2 otherwise skips encoder property subscriptions, so even actual IDRs have no V4L2 keyframe flag. The existing response handler maps this property to the flags FFmpeg and Sunshine use. Remove the patch when upstream reports encoder picture types correctly.

The cost is one separately built static FFmpeg/CBS package, two FFmpeg patches, and two Portal kernel patches. Frames use host memory rather than a zero-copy capture path. HDR, 4:4:4, AV1, and live bitrate changes are not supported by this profile. No software fallback is added. Remove each upstream backport when the reviewed dependency includes its fix; review the two local changes separately.

Physical probe and Moonlight results, including unresolved Wi-Fi packet loss, are recorded in [the Portal acceptance report](../../docs/acceptance/sunshine-korri-v4l2m2m-portal-2026-09-05.md). These results do not establish a production-ready host session or a sustained moving-content frame rate.

### Runtime settings patch series

Experimental live runtime-settings MVP split by review concern:

- `0001-add-runtime-settings-protocol-surface.patch` adds packet IDs `0x5504`/`0x5505`, runtime-settings operations, statuses, reasons, request/ack structs, and mail names.
- `0002-wire-runtime-settings-control-plane.patch` adds the Sunshine control-plane parser, `SUNSHINE_LIVE_SETTINGS_MVP=1` gate, capability acks, mutation acks, launch/current-applied baselines, and request queueing.
- `0003-apply-runtime-bitrate-and-fps-changes.patch` introduced safe rejection for active-stream operation `1` and supports operation `2`: set effective stream FPS at or below launch FPS using runtime frame pacing.
- `0004-add-proof-gated-runtime-resolution-apply-path.patch` applies operation `3` with same-or-smaller even dimensions whose aspect ratio matches the stream within the sub-pixel tolerance of even-integer rounding (same-ratio scaling, e.g. 854x480 on a 16:9 stream); genuinely different aspect ratios are still rejected so the game is never stretched. Refreshes touch mapping after apply, and is treated as supported for the validated Korri runtime profile. The same-ratio tolerance replaces the former exact same-aspect equality in `0002-wire-runtime-settings-control-plane.patch`.
- `0005-add-seamless-vaapi-runtime-bitrate-path.patch` enables operation `1` for the supported `h264_vaapi` path by mutating FFmpeg VAAPI rate-control private state, forcing an IDR, and avoiding encoder teardown/reconnect. With the explicit live-settings gate, H.264 queries the normal and low-power VAAPI entrypoints for actual CBR/VBR/AVBR support, preferring the normal entrypoint when both qualify and otherwise preserving Sunshine's upstream order. This avoids Intel's low-power path when it exposes only CQP. Capability publication separately inspects the active encoder session and omits operation `1` and its bitrate bounds when the selected VAAPI rate-control state is not actually mutable.
- `0016-add-seamless-nvenc-runtime-path.patch` extends operations `1`, `2`, and `3` to active Linux `h264_nvenc` sessions. Bitrate changes call the NVIDIA driver's synchronous reconfiguration function on Sunshine's existing encode thread, preserve the established VBV duration, force one IDR through NVENC, and acknowledge success only after the driver returns `NV_ENC_SUCCESS`. FPS continues to use the existing backend-neutral frame pacing. Resolution continues to use the existing fresh-frame replacement and capture-reinitialization path; the VAAPI destructor-flush exception remains VAAPI-only.
- Active-stream bitrate changes are advertised only for a live `h264_vaapi` or `h264_nvenc` session whose backend-specific mutable-rate-control guard succeeds; no reconnect or encoder-restart fallback is considered shippable.
- Runtime FPS and resolution are limited to active H.264 VAAPI or NVENC sessions.
- The series does not use the failed public AVCodec field/AVOption mutation fallback.

Runtime settings mechanism contract:

- Moonlight and Sunshine expose mechanisms and facts only; Korri owns adaptation policy.
- Runtime settings decisions distinguish local Moonlight command readiness, host Sunshine runtime-settings capability, and target-client proof as separate facts.
- Existing packet IDs remain stable: request `0x5504` and ack `0x5505`.
- Operation `0` is a non-mutating capability query for the active Sunshine session.
- Operation `0` returns a `0x5505` capability ack with gate status, reason, active-session supported operations, conservative bounds, launch baseline values, and current applied bitrate/FPS/resolution facts.
- Capability queries are coalesced to one pending record on the serialized control thread. The first query starts one 100 ms encoder-capability settling window, later queries replace only the pending request identity, and session removal drops the record. No detached thread, asynchronous timer, or cross-thread control-server send is used.
- Launch baseline bitrate, FPS, and resolution are tracked separately from current applied values for the lifetime of the stream.
- Restore is explicit: callers send normal set commands back to the launch baseline values; Sunshine does not auto-restore from network or command outcomes.
- Operations `1`, `2`, and `3` remain bitrate, FPS, and resolution mutation requests.
- Mutation acks carry the broad numeric status plus an additive reason field; current no-reason consumers must be updated before relying on reason-bearing payloads.
- Runtime resolution is a normal runtime-settings operation for the validated Korri profile; operation `0` advertises operation `3` only when the active session supports it.
- Capability support is conservative: active-stream FPS and resolution are advertised only for the explicit live-settings gate on an active H.264 VAAPI or NVENC session. Bitrate additionally requires the active backend's mutable-rate-control guard. Unsupported sessions return a reason without setting support bits.
- Operation `1` support requires same-session moving-video and bandwidth proof on the target client before it is treated as product-ready for that client/decoder combination.
- Operation `3` outcomes distinguish raw Sunshine ack state from caller-visible applied truth: Sunshine may report `server_applied=1`, while local-control must still expose applied width/height state for callers to verify.

VAAPI and NVENC runtime-bitrate maintenance policy:

- A stable FFmpeg helper/API is the preferred replacement for Sunshine-side private-struct mirroring, but Korri is not carrying that downstream FFmpeg API yet. The current path stays inside `sunshine-korri` because it has SM8550 evidence and avoids forking FFmpeg's encoder internals before an upstreamable helper shape is clear.
- The private mirrors are allowed only for the exact pinned FFmpeg/libavcodec version encoded in the patches. FFmpeg upgrades, including same-major minor/micro updates, must fail at compile/source-check time until the mirrored VAAPI and NVENC layouts are reviewed. The NVENC prefix is additionally compiled against exact FFmpeg commit `61c50407fd429a5e2ec616e2e846c3fe3743879a` and Sunshine's bundled NVENC API 12.0 headers, with offset checks for every accessed private field.
- The Linux-host module exposes `/run/opengl-driver/lib` only for the explicit `encoder = "nvenc"` policy and uses a systemd condition to require readable `libcuda.so.1` plus `libnvidia-encode.so.1` without a restart loop. It also enables strict encoder selection, so later CUDA, permission, API, or driver failures reject the stream instead of silently falling back to VAAPI.
- NVENC reconfiguration copies the active driver configuration before mutation, calls `NvEncReconfigureEncoder` synchronously, and commits FFmpeg/Sunshine applied state only after driver success. It adds no polling thread, detached work, lookahead, multipass, or automatic adaptation policy. Release-capable client range gestures dispatch only the newest value at the release edge, with one bounded fallback if release is lost. The server rate-limits every NVENC attempt from driver-call completion and rejects attempts inside a 500 ms per-session interval as conflicts so alternate clients cannot produce an IDR/reset storm. One failed call or one call exceeding 100 ms disables bitrate mutation for that encoder session while leaving FPS, resolution, and the stream available; a successful encoder replacement may republish support.
- Rollback remains the Nix-owned live-settings gate: disable `services.korriLinuxHost.sunshine.runtimeSettings.enable` to keep `sunshine-korri` deployed and omit `SUNSHINE_LIVE_SETTINGS_MVP=1`.

Runtime-resolution VAAPI destructor teardown policy:

- Runtime-resolution replacement crosses an encoder generation boundary: the replacement session is primed with the first post-switch frame and then becomes the active session.
- Destructor drain/flush is skipped only for the AVCodec session pair that participates in that runtime VAAPI replacement path: the outgoing generation and the primed replacement generation. Normal encoder sessions still drain on destruction.
- The evaluated alternatives are pre-drain, async teardown, skip-drain, and packet-drop alternatives. Pre-drain can re-enter the crashing FFmpeg hardware teardown after a generation break; async teardown moves the same lifetime hazard to another thread; packet drop risks hiding required end-of-stream data. Narrow skip-drain is the carried option until physical soak evidence proves a safer upstreamable teardown.
- Long-cycle no-crash and leak evidence belongs with the runtime-resolution hardware soak follow-up; source checks only prove the skip remains narrow and documented.

Runtime settings status contract:

- `0` — applied
- `1` — failed or unsupported
- `2` — invalid
- `3` — disabled

Reason codes:

- `none`
- `gate-disabled`
- `invalid-bounds`
- `invalid-payload`
- `unsupported-encoder`
- `unsupported-backend`
- `unsupported-operation`
- `apply-failed`
- `control-not-ready`
- `no-ack`
- `conflict`
- `stale-ack`
- `stream-ended`

Current automated gates:

- `nix build --no-link .#checks.$(nix eval --raw --impure --expr builtins.currentSystem).sunshine-korri-runtime-settings` checks the approved patch hashes, protocol constants, runtime safety rules, provenance, and the patched Sunshine build.
- `nix build --no-link .#checks.$(nix eval --raw --impure --expr builtins.currentSystem).sunshine-korri-input-seat-patch` checks the input-seat source boundary, the approved patch hash, the patched Sunshine build, and a nonblocking transport model/source-invariant check under receiver backpressure. The model does not execute compiled Sunshine behavior.
- `nix build --no-link .#checks.$(nix eval --raw --impure --expr builtins.currentSystem).sunshine-korri-android-client-protocol` checks the Sunshine and Android Moonlight protocol values together and runs host-native golden tests.

### Android Moonlight protocol mechanism

The Android Moonlight native layer implements the matching `0x5504` request and `0x5505` acknowledgement treaty. It exposes capability query, bitrate, FPS, and resolution commands plus one immutable non-secret version-2 snapshot through JNI. The snapshot has one monotonic session epoch and change generation, separate query and mutation terminal results, exact capability status and reason facts, a stale-ack counter, and a reconciliation-required fact. Terminal state remains visible until the next control-stream epoch starts.

One process-lifetime atomic lock serializes readiness, request acceptance, ENet enqueue, send-failure publication, snapshot reads, and stop. Control-stream teardown records the terminal state but does not remove the lock or the snapshot. The next control-stream epoch resets the state while it holds the same lock. This lock is not a Moonlight platform mutex, so platform cleanup does not count or delete it. The feature becomes active only for Sunshine on an encrypted ENet control stream with a connected peer. TCP, unencrypted, non-Sunshine, disconnected, and inactive calls do not submit a runtime-settings packet. Every terminal connection callback first marks the runtime-settings session inactive. The native state machine validates exact payload sizes, host-only status and reason combinations, capability masks, launch and current bounds, remaining timeout deadlines, clock regression, stale acknowledgements, and query-versus-mutation ordering. Unavailable capability acknowledgements keep zero operation masks and bitrate bounds but carry the nonzero launch FPS in `maxFps`, as the pinned host sends. Rejected mutation acknowledgements update the host-authoritative current value only when it remains inside the negotiated bounds. A pure 31-field serializer is shared by JNI and host-native index tests. The client does not select values or adapt the stream. Korri product policy remains outside the Moonlight transport layer.

The historical physical records from the legacy branch are not present in this branch. Import them only with a clear legacy provenance label. Current Intel VAAPI physical bitrate, FPS, resolution, lifecycle, touch, and soak evidence is recorded in [`docs/acceptance/sunshine-korri-physical-runtime-settings-2026-09-01.md`](../../docs/acceptance/sunshine-korri-physical-runtime-settings-2026-09-01.md). Current NVIDIA NVENC runtime, lifecycle, contention, and soak evidence is recorded in [`docs/acceptance/sunshine-korri-nvenc-physical-2026-09-01.md`](../../docs/acceptance/sunshine-korri-nvenc-physical-2026-09-01.md). The persistent headless moving-content follow-up, including 1080p60 transport/runtime-control acceptance, the rejected 1080p120 result, and the Android disconnect fix, is recorded in [`docs/acceptance/sunshine-korri-headless-real-consumer-2026-09-01.md`](../../docs/acceptance/sunshine-korri-headless-real-consumer-2026-09-01.md). Input-seat physical evidence remains a separate gate.

The ten-patch automated restoration record is preserved as historical evidence in [`docs/acceptance/sunshine-korri-automated-restoration-2026-08-31.md`](../../docs/acceptance/sunshine-korri-automated-restoration-2026-08-31.md). Current fourteen-patch NVENC, Wayland capture, protocol, provenance, and client evidence is recorded in [`docs/acceptance/sunshine-korri-nvenc-automated-2026-09-01.md`](../../docs/acceptance/sunshine-korri-nvenc-automated-2026-09-01.md). Automated evidence does not replace physical acceptance.

### Input-seat event mirror patch

`0015-add-korri-input-seat-event-mirror.patch` adds a native Sunshine event-source seam for Korri input seats. Sunshine remains an event source. It mirrors sanitized controller packets to a local Korri socket. A protected receiver must own emulator-visible virtual seats.

This patch is hardened from the legacy patch. It keeps the legacy wire schema. It does not keep the legacy blocking socket or loose sidecar parser.

The mirror needs these stable service values:

- `KORRI_INPUT_SEAT_MIRROR_SOCKET`: an absolute Unix socket path that the protected receiver owns.
- `KORRI_INPUT_SEAT_RUNTIME_DIR`: an absolute runtime directory that contains `sunshine-active-launch.json`.
- `sunshine-active-launch.json`: a root-owned sidecar with exactly `launchId`, `generation`, and `mirrorToken`. Its group must equal Sunshine's dedicated effective group. Group read is required. Group write, group execute, and every permission for other users are rejected. Mode `0640` is the intended producer mode.

The host service must keep the runtime directory and receiver socket root-owned. Sunshine receives only dedicated-group access. Sunshine opens the fixed sidecar with `openat`, `O_CLOEXEC`, and `O_NOFOLLOW`. It accepts only a root-owned regular file whose group equals Sunshine's effective GID and whose size is 4096 bytes or less. `launchId` and `mirrorToken` must be bounded strings. `generation` must be a nonnegative integer. Extra JSON fields are rejected.

Frames use bounded token-envelope NDJSON. A frame is 2048 bytes or less. The envelope contains `mirrorToken` and one `SunshineInputSeatFrame`. The frame can report `source-connected`, `source-state`, or `source-disconnected`. It does not contain keyboard, mouse, text, touch, pen, motion, battery, or raw device-path data.

Controller submission uses nonblocking AF_UNIX `SOCK_SEQPACKET` with `SOCK_CLOEXEC`. Each connection submits one bounded token-envelope NDJSON message. An incomplete send is a failure. The receiver must discard any message that does not contain exact JSON followed by exactly one terminal newline. Receiver backpressure cannot wait in the controller packet path.

The automated backpressure fixture remains a Sunshine-source transport check. The separate `korri-input-seat-receiver` repository check executes the compiled Rust receiver with its real `SOCK_SEQPACKET` control and mirror sockets and a non-privileged test backend.

`services.korriLinuxHost.sunshine.inputSeats.enable` activates the receiver path. The option is off by default. When enabled, Korrid acquires one exact launch lease before it starts the game. The root receiver has no network access. It keeps only `CAP_CHOWN` for its fixed socket and sidecar groups. It uses a closed device policy, has `/dev/uinput` as its only allowed device, and has `/run/korri-input-seat` as its only writable path. It creates four `Korri Seat P*` Xbox-compatible devices. Valid Moonlight buttons that are not present on the Xbox-compatible seat are ignored without dropping the supported buttons, triggers, or axes from the same state. When the protected receiver paths are configured, Sunshine sends controller packets to the receiver and does not create its native gamepad, even before a launch sidecar exists. Sunshine keeps its existing `/dev/uinput` access for non-seat keyboard, mouse, and touch injection. Only the receiver creates devices named `Korri Seat P*`. Games can read the Korri seat event nodes but cannot open `/dev/uinput` or the receiver runtime directory. Lease loss, exact stop, source disconnect, or receiver shutdown sends neutral state and removes the seats. The Korri Android client sends the latest active controller state every 250 ms and repeats a disconnect state four times. The receiver sends neutral state only after five heartbeat intervals without a state packet. This prevents a dropped nonblocking release packet from leaving a held control active without releasing a valid held control.

Korrid uses one private local control lease that is derived from the legacy pre-spawn `start(launchId)` and `stop()` calls. The fixed `SOCK_SEQPACKET` request is version byte `1`, operation byte `1` or `2`, and the exact 32-byte launch ID. The receiver checks the Korrid peer UID and GID. The reply contains only version, status, and reason bytes. It does not contain the mirror token. A closed lease removes the sidecar and all seats.

The mirror remains inert when the protected receiver or either stable environment value is absent. A missing sidecar also disables the mirror. Socket connection and write failures do not stop Sunshine controller handling. The mirror token is private local authority. It must not appear in public status, network RPC, logs, or committed evidence. Physical device acceptance remains separate from the repository implementation.

## Korrid certificate control

Patch `0020` adds an opt-in private adapter for Korrid-managed Moonlight trust. Sunshine consumes exactly one root-created systemd `SOCK_SEQPACKET` socket named `korri-certificate-control`. Sunshine verifies the configured absolute pathname, root ownership, and exact narrow mode before it serves requests. It does not create a TCP, HTTP, or web user interface. The adapter remains inactive when any socket-activation value or the exact expected Korrid UID and GID are absent.

Each accepted connection carries one JSON frame of at most 16384 bytes. The non-mutating `attest` operation compares an expected Sunshine UUID and returns only whether it matches. Provision and revoke also check `SO_PEERCRED`, the exact Sunshine host UUID, and one valid public X.509 PEM certificate. Provision is idempotent by the certificate's SHA-256 fingerprint. Revoke removes only entries with that fingerprint. Sunshine preserves its existing `root.named_devices` schema, replaces its state file atomically, and proves that the replacement live TLS verifier accepts every current client certificate before it reports success. If a failed mutation cannot restore the prior durable state, Sunshine terminates instead of serving HTTPS or certificate-control requests with uncertain authorization. Mutation replies contain only status, whether state changed, and Sunshine's public server certificate. Certificate bodies and state contents do not enter logs.

The adapter does not replace TLS. It replaces only the manual GameStream pairing ceremony. The Moonlight private key remains on the Android device, and Sunshine's private key remains in Sunshine's existing state directory. The NixOS socket unit and namespace isolation are supplied by the Linux host module in the consuming slice. Physical device acceptance remains separate.

## Package provenance

The installed package contains `share/korri/sunshine-korri/provenance`. This mode-`0444` file records:

- the provenance format,
- the package name,
- the approved platform build profile,
- whether the build contains CUDA support,
- the base Sunshine version,
- the independently approved base Sunshine source hash,
- the exact observed base Sunshine source store path,
- the exact observed base Sunshine derivation path,
- the reviewed libavcodec version,
- the exact reviewed FFmpeg commit and source hash,
- whether the V4L2 M2M FFmpeg profile is enabled,
- the reviewed `build-deps` commit and source hash,
- the V4L2 M2M FFmpeg patch name, hash, and patch-set hash,
- the reviewed NVENC API major and minor version,
- the Sunshine executable path,
- each ordered Korri patch name and SHA-256 value,
- one SHA-256 value for the complete ordered patch set.

Nix also exposes the provenance path, build profile, CUDA state, V4L2 M2M state, approved base source hash, observed base source and derivation paths, ordered patch names, and patch-set digests through package passthru values. `approved-patches.nix` is the independent approval record. The approved profiles currently cover x86_64 Linux with CUDA, aarch64 Linux with software encoding, and a separate aarch64 Linux V4L2 M2M profile. Package evaluation fails when the profile, base version, base source hash, one patch hash, or the ordered patch-set digest changes. The host module also requires the exact approved final derivation and output, so an `overrideAttrs` derivative cannot preserve trusted metadata while replacing the executable or patches. Deployment checks must use these values to attest the exact package. The manifest contains no secret or device-specific value.

## Removal/upstream policy

Remove or replace a carried patch when one of these becomes true:

1. Sunshine upstream accepts an equivalent feature.
2. Korri no longer needs the behavior.
3. A cleaner patch supersedes the current one.
4. The evidence shows the approach is unsafe or too narrow for continued carrying.
