# Slice D9 — the app binds to the test owner and lists a holder's game

Run on 2026-11-15 at 09:37 to 09:40 (aka local time), on aka as root (no
heavy work on zao since that day), from a copy of the tree at `7bcdb86c7`
(rebased onto `origin/main` `bed2ee4db`).

```sh
# in that copy, with HOME and XDG_STATE_HOME under /var/tmp/android-d9:
nix run .#android-app-holder-proof -- --out /var/tmp/android-d9/proof-5
```

No person took part. aka's k10temp read 64 °C at the start and 81 °C at the
end; other agents' builds ran on aka too, so timings mean little. The
fixture APK was
`/nix/store/fq5dwzv9nl2adiw29cnrrbl1304kfy57-korri-android-app-fixture-apk-0.1.0`,
built on aka and signed with a debug key made in the run's home. The
emulator was Android 12 (API 31), google_apis, x86_64, headless, 2 cores,
console port 5640, its own adb server on 5141, its data wiped. It and its adb
server were stopped at the end; no emulator process was left. All 15 checks
passed (`result.json`).

| Step | What happened |
| --- | --- |
| The APK | No dex, `hasCode=false`; `assets/relays.json` is `["ws://127.0.0.1:47800"]`; the x86_64 library carries the TLS verifier's classes and the Kotlin standard library |
| The sender | stream-cadence's sender fixture on aka, offering `Korri Stream` (`sender-facts.json`): Sunshine 2026.516.143833 on port 36431, hardware encoder on `renderD128`. Its unpaired `serverinfo` answered at `192.168.1.117:36431`, the holder's Moonlight address (`sender-serverinfo.xml`). Its restore check passed; no product unit changed and no input device appeared or went |
| The holder | The federation fixture on aka's loopback, port 38233, mapped into the emulator with `adb reverse`; its relay behind the app's port 47800 |
| The binding | `federation_fixture member` made a new device, `25b4c9be…`, bound to the test owner. The proof wrote its two identity files into the app's `no_backup/korrid-state/identity` through `run-as` before the first start (`seeded-identity.txt`: both `-rw-------`, the app's user) and deleted its own copy |
| The app's korrid | `korrid: Android TLS verifier classes loaded`. It published its owner statement for `25b4c9be…`, signed by `f9308a01…` (the test owner), and received the endpoint record that the holder `da65118a…` addressed to it (`relay-events.json`) |
| Velvet | 62.4 s after the start the readiness line said `page=home count=2 selected=1` (the game and the "All games" tile). korrid's "Identity connected" covered the library (`identity-connected.png`); the second Confirm opened it: the first injected key is lost (README, known gaps). `library.png`: "Acceptance Game", "Not played yet", source `federation-acceptance`, Play |
| korrid's memory | `federation/peers.json` in the app's state names the holder |
| Logcat | No fatal signal, panic or ANR (`logcat-app.txt`) |

`holder.log` shows korrid refusing its launch helpers because the holder ran
as root (README, known gaps). Nothing here starts a launch; D10 does.
