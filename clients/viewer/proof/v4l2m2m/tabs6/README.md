# tabs6 — installed viewer picture pass

Verified on 2026-11-24. This is a direct installed-viewer/private-fixture proof.
Production korrid launch and controller routing remain deferred to land.

| Observation | Result |
| --- | --- |
| Installed provenance | Signed aarch64 archive; enabled; exact owner approval |
| Decoder | `h264_v4l2m2m`, `/dev/video0`, `iris_driver` / Iris Decoder |
| Presenter | `Platform V4L2 M2M + SDL NV12`; `presenter=sdl-nv12` |
| Picture | 2560×1600; readable source IDs 616, 2574, 4428 |
| Screenshot times after host observed first draw | 7.190 s, 40.672 s, 72.416 s |
| Moving-picture evidence span | 65.225 s |
| Bounded run | 82.725 s total; clean `exit=0`; normal stream/platform cleanup |
| Thermal gate / abort | Every handheld zone below 75 °C / abort at 85 °C |
| Start / peak / later after-run maximum | 49.0 / 61.6 / 44.8 °C; all 24 zones sampled |
| Network drops reported by viewer | 62 frames; IDR recovery visible in log |
| Exact decoded/presented totals | Not available; CSV has header only |
| Presenter drops / frame pacing | Not measured |
| Audio | 1 kHz source tone; viewer FL/FR linked to Speaker playback; hardware PCM RUNNING |
| Acoustic output | Not measured |
| Product game on aka at begin | None |
| Aka product units | PIDs/start times unchanged |
| Device system generation | Unchanged across stream: `0ghaibw1bh2z58ysbk2dvsszdgp4zyvv` |
| Cleanup | No viewer remains; temporary client keys removed; source fixture removed; scoped firewall rules removed |

See `picture-result.json`, `viewer-command.json`, `viewer.log`, `thermal.tsv`,
`stop.txt`, the three `stream-*.png` files, `audio-during.txt` and
`pipewire-during.txt`. `cleanup-processes-and-temperature.txt` and
`postflight.txt` were collected after the run; they are not immediate
post-stop samples. Preflight screenshots show the original shell, not the
viewer, and do not contribute to the verdict.

The initial system preflight predates another session's system deployment.
This work did not deploy a system generation. The per-stream
`system-before.txt` and `system-after.txt` are the relevant unchanged-system
check.

`attempts/pair-timeout/` was refused by aka's existing private-port firewall.
`attempts/capture-and-stop/` decoded pictures but had harness-only capture,
fullscreen and wrapper-stop defects. The final run used the same signed
viewer, direct child PID tracking, a PID-scoped fullscreen/focus
operation, and an already-evaluated capture program. No C decoder policy was
changed between attempts.
