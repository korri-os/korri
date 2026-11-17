# #476 Route C — korrid carries a live resize to the holder's Sunshine, and the viewer reports `ready`

**Passed.** On the emulator, the app's korrid asked the holder for its
stream's portrait size. The holder found the stream through the app's
certificate grant and asked its Sunshine on the certificate-control socket.
Sunshine (patch 0046) swapped the encoder, and the app's picture went on at
720x1280, then back at 1280x720. Since the follow-up to slices 2 and 3, the
viewer reports its shape to korrid with `ready: true`, and its `encoded`
size follows each change in the same connection. All 35 checks passed
(`result.json`).

**Not run live:** korrid's Portrait toggle and the holder's display change
(the Sway mode and the display hooks). The holder is korrid's federation
fixture, which has no compositor, so its `viewer_display_report` refuses
before any encode size, and korrid then lists no Portrait control. The
proof shows the input that enables Portrait: korrid's own read of the
viewer's snapshot says `ready: true`. The proof trigger stands in for the
toggle's last step: the same request to the holder (`holder_resize` in
`services/korrid/src/upstreams/live_shape.rs`). The toggle, the holder's
transaction and their encoder step, and that a ready viewer's Portrait is
enabled, are covered by unit tests only (`services/korrid/src/upstreams.rs`
and `src/host/client_shape_tests.rs`).

**Known limit:** a grant names the certificate that the peer sent in its
provision. Nothing proves that the peer holds that certificate's private
key, so a grant does not prove that the caller is the stream's TLS client
(`clients/android-app/README.md`, "korrid's resize request").

## The run

2026-11-17, 02:27 to 02:30 UTC, on aka, as root, with `nix run
.#android-app-resize-proof -- --build-store auto` (`proof/resize.py`). aka
read 64.1 °C before the run; no other emulator ran, and the product's
`korri-sunshine` had no connection.

| Part | Value |
| --- | --- |
| korrid | This branch: one certificate-control operation at a time, the provision grant rule |
| App | The fixture APK, `/nix/store/rdzsfb2a2krhb63ixdgj76mnl2837bsa-korri-android-app-fixture-apk-0.1.0`, with the cargo feature `proof` (and korrid's `proof`); its viewer names its screen in Start and reports `ready` |
| Holder | korrid's federation fixture, `/nix/store/524p0cslbrhzc9h7bsg8psqsvagsy1dh-korrid-federation-fixture-0.0.0`, as uid 59034 with no account |
| Sender | stream-cadence's sender fixture with `sunshine-korri` from korri-plugins `86f6236`, `/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri`, patch set `9aefa180…`: 11 patches, `0046-resize-a-stream-for-korrid` last (`sender-facts.json`). Encoder `h264_vaapi`, output 1280x720 |
| Emulator | Android 12 (API 31), x86_64, headless, 2 cores, window 2400x1080 |

## Results

| Step | Result | Evidence |
| --- | --- | --- |
| korrid provisions the viewer's certificate through the holder, which records its grant | yes | `logcat-app.txt`: "viewer certificate provisioned through korrid's broker"; Sunshine holds one client |
| The viewer names its screen | yes | `logcat-app.txt` 02:29:36.130: Start with `display` 2400x1080. The window was 1080x2400 then (the library is portrait), and the screen is its longer side first |
| The picture moves at 1280x720 | yes | `stream-1.png`; frame IDs 5490, 5583, 5675, 5778 |
| **korrid reads `ready: true`** | **yes** | korrid's live-shape poll, 02:29:46.769: `viewer snapshot for korrid: … "screen":{"width":2400,"height":1080},"encoded":{"width":1280,"height":720},"ready":true`; the trigger's snapshot says the same (`result.json`, `korrids_snapshot_shape`) |
| `resize LAUNCH 720 1280` answers `Applied` | yes, in 0.57 s | `logcat-app.txt`: `korri-app: proof: resize … 720x1280 Applied` |
| Sunshine took korrid's request, not a client's | yes | `sunshine.log` 02:29:57.377: `Runtime resolution applied: 720x1280`, `Korri certificate control: resize to 720x1280 applied` |
| The decoder and the video layer follow | yes | `decoded size 720x1280`, video layer placed at (896, 0)-(1504, 1080) in the window 2400x1080 |
| **korrid reads the new encoded size, in the same connection, still ready** | **yes** | `picture: the picture shown is 720x1280` at 02:29:57.842, the Snapshot event after it, and the trigger's snapshot: `encoded` 720x1280, connection `3098:120:567405988`, `ready: true` |
| The picture goes on at 720x1280 | yes | `portrait-1.png`, `portrait-4.png`; frame IDs 6055, 6137, 6223, 6308, in the new letterbox |
| `resize LAUNCH 1280 720` answers `Applied`; the picture and korrid's `encoded` go back to 1280x720 | yes, in 0.58 s | `sunshine.log` 02:30:06.854; `landscape-1.png`, `landscape-4.png`; frame IDs 6570, 6668, 6761, 6863; the trigger's snapshot: `encoded` 1280x720 in the same connection |
| No crash, panic or ANR; the fixture restored aka | yes | `result.json` |

The two clocks differ: `logcat-app.txt` has the emulator's time and
`sunshine.log` has aka's.

Earlier runs of 2026-11-16 (09:34 and 10:38 UTC) passed 29 checks of the
same steps without the viewer's shape: the app's Start named no screen, so
its viewer reported no shape and korrid's live-shape never read it. This
run replaces them.

## The path

1. The trigger (`src/proof.rs`, `resize LAUNCH W H`) calls
   `korrid::android_app::proof_resize_stream`, which posts
   `app.session.viewer.resize` to the app's own korrid, so that the peer call
   runs on korrid's runtime. Only a build with the feature `proof` answers
   it there (`services/korrid/src/lib.rs`).
2. The app's korrid sends the same request to the holder over the peer wire
   (`NativeClient::viewer_resize`).
3. The holder checks the exact launch and that the caller takes part in it,
   reads the caller's `CertificateGrant`, waits for its turn on the socket,
   and sends `resize` to Sunshine's certificate-control socket
   (`host/moonlight_certificate.rs`).
4. Sunshine matches the certificate's fingerprint to the running session and
   gives the request to that session's patch 0044 channel; the answer goes
   back to korrid.

Not shown here: a provision or a resize that waits for its turn behind
another operation, and the holder's answers when it reads no answer from
Sunshine. Unit tests cover those against a real seqpacket server and the
peer RPC (`host/moonlight_certificate.rs`, `peer_rpc.rs`, `lib.rs`).

## Rerun

Build on aka, copy the tree there, and run as root:

```sh
KORRI_ROOT=$PWD nix run .#android-app-resize-proof -- --build-store auto \
  --sunshine /nix/store/...-sunshine-korri-2026.914.233613-korri \
  [--apk FIXTURE.apk] [--fixture federation_fixture] --out DIRECTORY
```

`--build-store auto` builds in aka's own store (#500); without it the proof
builds nothing and refuses what is in neither aka's store nor a cache. The
run above was before #500, with no such option.

The Sunshine package must carry patches 0020 and 0046. The proof waits
while the sender fixture's heat guard refuses.
