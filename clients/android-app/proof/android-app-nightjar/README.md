# The Korri app on nightjar-core: the play proof six times (#391 step E)

**All six runs passed all 127 checks**: five with nightjar-core, the app's
default network core since commit "the Korri app streams with nightjar-core
by default", and one with the C core through the fixture's proof file
(`--network-core c`). Each run covers D10 to D11 and what came after them:
the tap that makes korrid start the viewer, korrid's certificate, the
stream, keys and a tap at the sender, the sound, Korri's gameplay overlay
with the cutout and the turns, the stream's controls (Crop to fill, Toggle
keyboard, Mouse mode, Pan & zoom), Home, and the stream again.

Run on aka as root, 2026-11-18 14:51 to 15:39 UTC, from a clean clone of
that commit as it was then (`c45d0eb86`), with the fixture APK and the
federation fixture built there. The commit was then changed only by
rustfmt, in one log call of the fixture's `src/proof.rs`, so the store
paths below are that tree's:

```sh
# in the clone, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/step-e-core/play-home:
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5674 --adb-port 5174 [--network-core c] --out /var/tmp/step-e-core/play-proofs/run-N
```

| Store path | |
| --- | --- |
| Fixture APK (unsigned) | `/nix/store/9rbaf6x7lql9syz222ay7ygz7wrkc73g-korri-android-app-fixture-apk-0.1.0` |
| Federation fixture | `/nix/store/8vxzin1r8q7mjd88s8yrk65ck3ihbwsy-korrid-federation-fixture-0.0.0` |
| Product APK (unsigned) | `/nix/store/nf6sg5v0fk9954s3xdk5svls0jcin5b9-korri-android-app-apk-0.1.0` |
| Release APK (unsigned) | `/nix/store/lbr7j8x0s4n7m91xl70k15llg5pzx9k7-korri-android-app-release-apk-0.1.0` |

No person took part. The six runs started after three checks a minute
apart found aka quiet, and held `/var/tmp/korri-turns.lock` throughout.
Other sessions' work started after the gate and did not wait for the lock:
a game binary of user `korri` (`b5244dfc8e49df2…`, 156 to 213 % CPU) and
`grim` from run 1 on, and an aarch64 cross build (`nixbld10`, `cc1`) in runs
4 and 5. aka's 1-minute load was 0.9 at run 1's start and 14.6 to 22.7 at
the later starts; Tctl 71 to 96 °C (`host-before.txt`, `host-after.txt` in
each run). Each run waited for the sender to cool before it prepared it
(the proof's `sender_cooldown`: 10 to 30 s, and 410 s in run 5). Run 5's
first try waited the whole 600 s and was refused for heat
(`refused-run-5-try-1/`); its second try is run 5. These runs prove the
app's function under load, not its numbers.

| Run | Core | Checks | Stop line at Home | Sender cooldown | Tctl before, after |
| --- | --- | --- | --- | --- | --- |
| 1 | nightjar | 127 of 127 | `nightjar: at the stop` | 20 s | 71.4, 87.5 °C |
| 2 | nightjar | 127 of 127 | `nightjar: at the stop` | 30 s | 86.8, 90.5 °C |
| 3 | nightjar | 127 of 127 | `nightjar: at the stop` | 10 s | 89.6, 92.9 °C |
| 4 | nightjar | 127 of 127 | `nightjar: at the stop` | 20 s | 92.4, 95.9 °C |
| 5 | nightjar | 127 of 127 | `nightjar: at the stop` | 410 s | 95.8, 90.0 °C |
| c | C | 127 of 127 | `Stopping input stream...` | 30 s | 89.3, 91.8 °C |

The two checks that this commit adds pass in every run:
`library_carries_nightjar_core` (the fixture library holds the nightjar
adapter's stop line) and `the_stream_used_its_network_core` (the stream's
stop lines are the asked core's and no other's). In run c the app logged
`korri-app: proof: network core c for this stream` at each start.

The input report at the end of each run: `12 keys, 0 controller states, 0
arrivals, 14 to 23 mouse, 0 touches sent; 0 held; 6 unsupported, 0 outside
the stream, 0 failed`, with both cores. The 6 unsupported are touches that
this Sunshine does not take (the C core's earlier D10 records have the same
answer). No run logged "Input queue reached maximum size limit".

Records: `run-{1,2,3,4,5,c}/`: the proof's text and JSON files (its
`result.json`, the app's logcat lines, Sunshine's log and input packets,
the holder's logs, the relay's events, the seats, the displays' cutout
dumps), its output (`proof.log.txt`) and the host's state. Left out: the
APKs, the WAV file, the screenshots, the full logcat, the agent's log, and
the input method's and SurfaceFlinger's dumps.

## The release APK's notices

The release APK built from the same commit opens its notices with the app's
licence, GPL-3.0-only, for its viewer's Artemis parts, its network cores
nightjar-core and moonlight-common-c, and the Rubylight crates. A part of
its own names nightjar-core, rubylight-protocol, rubylight-traverse and
rubylight-native-api under GPL-3.0-only, with their projects and Korri's
copies in `clients/viewer/vendor`; cargo-about lists nightjar-core,
rubylight-protocol and rubylight-native-api under GPL-3.0-only and
Nightjar's `rusty_enet` under MIT, with the new crates.io crates (`rsa`,
`pkcs1`, `pkcs8`, `spki`, `der`, `const-oid`, `num-bigint-dig`, `num-iter`,
`signature`, `spin` 0.9.9 and `socket2` 0.5.10). The arm64 library's build
checked that it holds `nightjar_core::session::start` and the ARMv8 AES and
POLYVAL code.
