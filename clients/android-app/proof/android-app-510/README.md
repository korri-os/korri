# #510 — the certificate step beside an owner device that never answers

The owner's phone spent 5.04 s in the certificate step of every stream's
start (2026-11-19, about 06:48 UTC, a stream from aka):

| Mark (the app's logcat) | ms |
| --- | --- |
| `korrid starts the viewer` | 257613335.8 |
| `client certificate read` | 257613341.2 |
| `server certificate provisioned` | 257618384.7 |
| `Start` | 257618404.9 |
| `first frame shown` | 257618738.2 |

## The cause

korrid's broker (`UpstreamRegistry::moonlight_native_peer`) asked every
owner device to attest the Sunshine host UUID, each for at most 5 s
(`NATIVE_RPC_TIMEOUT`), and waited for all of the answers. A device with
no live route never answers: its call waits in `reach.wait` for a
connection (20 s at most) until the 5 s bound ends it. So one such device
held every start for 5.0 s, though the holder answered at once. The step
on the phone is those 5.0 s and 44 ms for the rest.

What the phone and aka showed, read on 2026-11-19 at about 13:55 UTC:

- The phone's korrid remembers 10 owner devices
  (`no_backup/korrid-state/federation/peers.json`); 4 of them have no
  endpoint event at all.
- aka's korrid journal for the same minute: 8 `korrid save push:
  SavePeerUnavailable` lines at 06:48:03, one for each owner device whose
  save push failed then (`host/saves.rs`). It has no line for a
  certificate request; the holder logged none before this change.
- The phone's logcat no longer had the launch (its buffer began at 12:59
  UTC), so no line of the phone shows the attests themselves.

The holder's part was not the cause. The broker sends the provision only
after the attests, so the holder's device lock, #502's retire-first, its
grant and Sunshine's answer fit in the 44 ms that remain. That is read
from the broker's order and the phone's marks, not from a line of aka.

## The proof on aka's emulator

`play.py --offline-peer` gives the app's korrid a second owner device that
never answers: the fixture's member owner statement of another device, on
the holder's relay, with no endpoint. Two runs on 2026-11-19, on aka as
root, from a clean copy of the branch at `b67ff9eed`, on nightjar-core. The
holder was this branch's korrid in both runs; only the app differs.

```sh
# in that copy, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/cert5s/play-home:
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5674 --adb-port 5174 --offline-peer --out DIR [--apk APK]
```

| Run (aka's clock, UTC) | The app | Certificate step | Checks |
| --- | --- | --- | --- |
| Before, 14:18–14:23 | `origin/main`'s fixture APK (`7565d2aea`), `/nix/store/97kd4vbsw87h2x143igf03jasyapgn2j-korri-android-app-fixture-apk-0.1.0` | **5 457.3 ms** | 153 of 155: `the_certificate_step_waits_for_no_offline_peer` and `the_certificate_step_is_shorter_than_an_attest_bound` failed, as they must |
| After, 14:23–14:29 | This branch's fixture APK, `/nix/store/9wnrclsjyr1q4hvlk1wdpa4qyy1vp0zr-korri-android-app-fixture-apk-0.1.0` | **419.5 ms** | 155 of 155 |

The step runs from `client certificate read` to `server certificate
provisioned` (`before.json`, `after.json`: each run's failed checks, its
builds, its offline device, its certificate step and its start's
timeline, from `result.json`). The after run's korrid logged
(`logcat-app-after.txt`):

```text
korrid: certificate peer: 584b062a attested in 3 ms as the stream's holder; 1 of 2 peers not waited for
korrid: certificate provision at 584b062a: Ok in 3 ms
```

and its holder logged `korrid: certificate provision for peer 067131eb: Ok
in 0 ms` for the first start and `… Ok in 1 ms` for the start after Home.
The rest of the emulator's 419.5 ms is before the broker asks: the read of
Sunshine's `serverinfo` and the app's call to its own korrid.

No person took part. The run held `/var/tmp/korri-turns.lock` (a noclobber
file with its PID, and `flock`) from 14:18:00 to 14:29:18 and removed it.
At 14:18:00 aka read a load of 0.66, k10temp 57 °C and no emulator; at
14:23:51, between the runs, a load of 15.74 and 89 °C. The sender's agent
read 76.5 °C and 81.1 °C at its own cooldown checks, below its 85 °C start
limit. The emulator was Android 12 (API 31), google_apis, x86_64, headless,
2 cores; it and its adb server stopped with no process left
(`emulator_stop` in `before.json` and `after.json`). The product's
korri-sunshine and korrid on aka were not touched.

An earlier pair with the same two APKs, at 09:13–09:26 UTC and a load of 6
to 12, read 5 957.7 ms and 967.9 ms; its records are not kept here.

## The tests

`upstreams::tests` has three tests of the broker. With `origin/main`'s
broker and these tests, two fail: `the broker waited 5.001741272s`, and a
second device that attests the same UUID is still ambiguous. The third
keeps the old rule when the holder does not attest. With the fix all three
pass, and the full `korrid-check` passes on aka as `simon` in a network
namespace with `lo` and a dummy default route, with cargo offline: 2 299
tests passed, none failed, 27 ignored. That copy had `main`'s rustfmt
layout of `services/korrid/src/remote_signer.rs`; `origin/main`'s layout
of that file fails `cargo fmt --check`, and this branch does not change it.

Not proven: the owner's phone with this build. The phone's next start
shows the result in its logcat: `korrid: certificate peer: … attested in …
ms as the stream's holder`, and the step's two marks.
