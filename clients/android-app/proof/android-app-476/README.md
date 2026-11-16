# #476 Route C, slices 2 and 3 — korrid carries a live resize to the holder's Sunshine

**Passed.** On the emulator, the app's korrid asked the holder for its
stream's portrait size. The holder found the stream through the app's
certificate grant and asked its Sunshine on the certificate-control socket.
Sunshine (patch 0046) swapped the encoder, and the app's picture went on at
720x1280, then back at 1280x720. All 29 checks passed (`result.json`).

**Not run live:** korrid's Portrait toggle and the holder's display change
(the Sway mode and the display hooks). The holder is korrid's federation
fixture, which has no compositor, so its `viewer_display_report` refuses
before any encode size. The proof trigger stands in for the toggle's last
step: the same request to the holder (`holder_resize` in
`services/korrid/src/upstreams/live_shape.rs`). The toggle, the holder's
transaction and their encoder step are covered by unit tests only
(`services/korrid/src/upstreams.rs` and `src/host/client_shape_tests.rs`).

**Known limit:** a grant names the certificate that the peer sent in its
provision. Nothing proves that the peer holds that certificate's private
key, so a grant does not prove that the caller is the stream's TLS client
(`clients/android-app/README.md`, "korrid's resize request").

## The run

2026-11-16, 10:38 to 10:41 UTC, on aka, as root, with `nix run
.#android-app-resize-proof` (`proof/resize.py`). aka read 61.6 °C before
the run; no other emulator ran, and the product's `korri-sunshine` had no
stream.

| Part | Value |
| --- | --- |
| korrid | This branch, with one certificate-control operation at a time and the provision grant rule |
| App | The fixture APK, `/nix/store/9avkx39mjc6dfhl3q7x23l3y9msknj2i-korri-android-app-fixture-apk-0.1.0`, with the cargo feature `proof` (and korrid's `proof`) |
| Holder | korrid's federation fixture, `/nix/store/ydh0rhkhwc1qqdwp6jrw7rkjp5zbdcvq-korrid-federation-fixture-0.0.0`, as uid 59034 with no account |
| Sender | stream-cadence's sender fixture with `sunshine-korri` from korri-plugins `86f6236`, `/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri`, patch set `9aefa180…`: 11 patches, `0044-live-resolution-control`, `0045-send-https-close-notify-without-waiting` and `0046-resize-a-stream-for-korrid` last (`sender-facts.json`). Encoder `h264_vaapi`, output 1280x720 |
| Emulator | Android 12 (API 31), x86_64, headless, 2 cores, window 2400x1080 |

## Results

| Step | Result | Evidence |
| --- | --- | --- |
| korrid provisions the viewer's certificate through the holder, which records its grant | yes | `logcat-app.txt`: "viewer certificate provisioned through korrid's broker"; Sunshine holds one client |
| The picture moves at 1280x720 | yes | `stream-1.png`; frame IDs 5498, 5593, 5689, 5783 |
| `resize LAUNCH 720 1280` answers `Applied` | yes, in 0.66 s | `logcat-app.txt`: `korri-app: proof: resize … 720x1280 Applied` |
| Sunshine took korrid's request, not a client's | yes | `sunshine.log` 10:41:12.681: `Korri certificate control: resize to 720x1280 applied`, after `Creating encoder [h264_vaapi]` and `Runtime resolution applied: 720x1280` |
| The decoder and the video layer follow | yes | `picture: the stream's size changes from 1280x720 to 720x1280 at frame 600`, `decoded size 720x1280`, `video layer 720x1280 placed at (896, 0)-(1504, 1080)` |
| **The picture goes on at 720x1280** | **yes** | `portrait-1.png`, `portrait-4.png`; frame IDs 6034, 6124, 6212, 6298. Sunshine keeps the 1280x720 output's shape inside the portrait stream, so the bar's cells are 15 px at y 454, as the geometry predicts |
| `resize LAUNCH 1280 720` answers `Applied`, and the picture goes on at 1280x720 | yes, in 0.59 s | `sunshine.log` 10:41:21.408; `landscape-1.png`, `landscape-4.png`; frame IDs 6529, 6623, 6715, 6807 |
| No crash, panic or ANR; the fixture restored aka | yes | `result.json` |

The two clocks differ: `logcat-app.txt` has the emulator's time and
`sunshine.log` has aka's.

An earlier run on the same day (09:34 UTC) passed the same 29 checks with
the build before the socket turn, patch 0045 (close-notify) and the
renumbering of this patch from 0045 to 0046. This run replaces it.

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
