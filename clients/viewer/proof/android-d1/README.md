# Slice D1 — the Rust viewer app on the Android emulator

Run on 2026-11-14 at 09:42 to 09:44 (zao local time) from commit
`0581898e60a0258dfb360ac4536b9b62a0c31493`, with a clean tree:

```sh
nix run .#viewer-android-proof -- --out /tmp/korri-d1/proof-final
```

No person took part. The tool built everything on zao. The emulator was
Android 12 (API 31), google_apis, x86_64, headless, 2 cores, on zao. The
sender was stream-cadence's private Sunshine fixture on aka, reached at
aka's Tailscale address `100.117.97.45`, port 56687. The emulator was
stopped at the end; no `qemu-system` process was left.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/fb9b5q46s2vc3ai51cilc092rpwm71qj-korri-viewer-android-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/kyagfrpbsbdcafl8svvhcvzvgbaazkcc-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/isjr2vij8b65gll5c09d93jg69lvjyns-korri-viewer-android-check` |

## Checks

All 14 passed (`result.json`), and the fixture's restore check passed: no
process left, run folder removed, no live game.

| Check | Evidence |
| --- | --- |
| The APK has no code and both ABIs | `apk-contents.txt` (no `.dex`; `lib/arm64-v8a`, `lib/x86_64`), `apk-manifest.txt` (`hasCode` false), `apk-badging.txt` |
| The session part's tests pass on Android | `android-test-https.txt` (4 tests), `android-test-trust_anchor.txt` |
| Running event, then the Start reply `Ok` | `logcat.txt` 09:43:38.976, 0.6 s after Start |
| The app is in front while it streams | `resumed-while-streaming.txt`, `streaming.png` |
| Sunshine has the session while the app streams | `sunshine.log`: `New streaming session started`, `CLIENT CONNECTED` at 16:43:39 UTC |
| Back: Stopped event, Stop reply `Ok`, `viewer ended` | `logcat.txt` 09:43:57.274 to .301 |
| The activity finished after Back | `resumed-after-back.txt` (launcher), `after-back.png` |
| Sunshine saw the end | `sunshine.log`: `CLIENT DISCONNECTED` at 16:43:57.306 UTC |
| No crash | no `Fatal signal`, `FATAL EXCEPTION` or `panic:` in the whole logcat |

`streaming.png` is black on purpose: D1 has no picture. The core received
video ("Received first video packet after 0 ms") and its empty decoder
dropped it. The fixture sends no sound ("No audio traffic was ever received
from the host!").

## Trust anchor before and after the reset

The same emulator ran `tests/trust_anchor.rs` built twice for x86_64
Android, once without the `CURLOPT_CAPATH` reset in `Transport::new` and
once with it (`trust-anchor-before-the-reset.txt`,
`trust-anchor-after-the-reset.txt`). Before: the server whose certificate
came from the CA in the probed directory was accepted. After: refused.

## Not proven here

- The arm64 library on a phone. No device took part.
- Speed. Numbers from an emulator mean nothing (ADR 0063).
- Picture, sound and input: slices D2 to D4.
