# Slice D10 — a tap on the holder's game plays it in the app

Run on 2026-11-15 at 14:15 to 14:19 (aka local time), on aka as root, from a
copy of this branch at `9422bd8df`, on `origin/main` `05fd8a897`. The branch
was rebased onto D4b (`bea90e4ec`) after the run. The commits after this
record ran in no emulator: the app's start of D4b's controller feedback,
and the review's fixes (queued launches refused when the activity ends, a
panicked runtime, Android's keys while a stream stops, stale pin files).

```sh
# in that copy, with HOME and XDG_STATE_HOME under /var/tmp/android-d10:
nix run .#android-app-play-proof -- \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5650 --adb-port 5151 --out /var/tmp/android-d10/proof-final
```

No person took part. aka's k10temp read 64 °C at the start and 90 °C at the
end. Other agents' builds and sender fixtures ran on aka too, so timings mean
little. The fixture APK was
`/nix/store/5mww4vj4vzsvr69kd7p615rb3cxrj5xg-korri-android-app-fixture-apk-0.1.0`,
built on aka from that tree and signed with a debug key made in the run's
home. The emulator was Android 12 (API 31), google_apis, x86_64, headless,
2 cores, console port 5650, its own adb server on 5151, its data wiped. It
and its adb server were stopped at the end, with no process left. All 35
checks passed (`result.json`).

| Step | What happened |
| --- | --- |
| The sender | stream-cadence's sender fixture on aka (`sender-facts.json`): Korri's Sunshine `2026.914.233613-korri` with patch 0020, on port 42751, offering `Korri Stream`, `h264_vaapi` on `renderD128`. It had a certificate control socket of its own for the holder's group, and a 1000 Hz tone |
| The holder | The federation fixture (`/nix/store/2qnpvjlbim4f9hrs0hbjhlpjz14yr4s0-korrid-federation-fixture-0.0.0`, feature `fixture`) as uid 59034, gid 59034, with no account. Port 36769, device `7dd3cb3f…`. Its launch units were in memory; its input seats went to the proof's own receiver (`seat-receiver.jsonl`: `hello`, `applyCount 4`, `beginSession` for the launch, the lease, `route`), never to aka's |
| The binding | `federation_fixture member` made `a7ae2b36…`, bound to the test owner, seeded into the app's private state as in D9 |
| Velvet | 62.0 s after the start it listed "Acceptance Game"; the proof found its tile and Play on screen (`library.png`) |
| The tap | korrid started the viewer 1.7 s after the tap on Play: `viewer certificate provisioned through korrid's broker for 192.168.1.117:42751` (14:17:44.422), then Start for launch `ee96ecba…` (14:17:44.613) |
| Running | `viewer event {"kind":"Status","value":"Running"}` at 14:17:55.114, 12.2 s after the tap; `viewer status for korrid: … Running` at 14:17:55.186. Sunshine held one paired client, the one korrid provisioned |
| The picture | The decoder was `c2.goldfish.h264.decoder` at 1280x720, in landscape. The frame-ID bar rose: 5481, 5615, 5739, 5862 in `stream-1.png` to `stream-4.png` |
| The overlay | Back opened the stand-in at 14:18:06.391 (`overlay: open on Back; the stream's input is held`). It darkened the lower half from a mean of 122.0 to 26.3 (`overlay-1.png`), and the picture went on under it: 6068, 6191, 6332. The second Back closed it 8.5 s later; the picture went on (6483) |
| The input | The keys after the marker reached Sunshine (`M K K O O R R I I`, `sunshine-input.json`), and the tap reached it as absolute mouse moves and a mouse button |
| The sound | The emulator recorded 37.6 s, 34.0 s of it at 1000 Hz (median RMS 258.9), nothing at 440 Hz |
| Home | The launcher came up (`home.png`). `viewer stops: the app's window went` at 14:18:24.764, Stopped at 14:18:25.024, korrid read Stopped at 14:18:25.026, and the layers went after the runtime ended |
| Back to the app | korrid's recovery provisioned the certificate again (14:18:29.061) and started the same launch; Running at 14:18:38.901. The bar rose again: 7953, 8080, 8197 (`again-3.png`) |
| Logcat | No fatal signal, panic or ANR (`logcat-app.txt`) |
| The sender after | Its restore check passed: no product unit changed, no live game, no input device appeared or went, and its run folder was removed |

`holder.log` shows one refusal that does not matter here: the holder cannot
read the device limits as an unprivileged user.

An earlier run with the same app code (proof-7) failed at Start: Sunshine
answered RTSP ANNOUNCE with 500 while another agent's sender fixture ran on
aka. That run kept no Sunshine log, so the cause is unknown. The proof keeps
`sunshine.log` since then, also after a failure.
