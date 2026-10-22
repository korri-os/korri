# RG35XX SP Sunshine / Cedrus H.264 — acceptance, 2026-10-22

The RG35XX SP (V1 panel) passes the same encoder probes as the RG35XX Pro and
streams to Moonlight with the hardware encoder. It shares the Pro's kernel,
encoder patch 0237 and limits; see
`sunshine-korri-rg35xxpro-v4l2m2m-2026-10-22.md` and
`sunshine-korri-rg35xxpro-frame-profile-2026-10-22.md`.

## Device and image

| Item | Value |
|---|---|
| Device | Anbernic RG35XX SP, V1 panel (`sun50i-h700-anbernic-rg35xx-sp.dtb`) |
| Image | `packages.aarch64-linux.rg35xxsp-sd-image` from branch `feat/rg35xxsp-sunshine` (commit `50a2e164b`, on `89647ca79`) |
| Image store path | `/nix/store/fir1gk1r7rrsxhq4lvxih0w9vak0iacd-nixos-rg35xxsp.img.zst` |
| Card check | first 7,182,200,832 bytes of the card SHA-256 `bf1e876d3f684f815d582969acb35b0685f75a1932fcee7ca9b8843258f8095d`, equal to the decompressed image |
| Booted system | `/nix/store/7547xzyd0fpvqn0azzci0lb5yg3hbccd-nixos-system-rg35xxsp-sd-card-26.11.20260923.4975466` |
| Kernel | 7.2.0; `sunxi-cedrus` from `/run/booted-system`, no module replaced |
| Video nodes | `/dev/video0` `cedrus`, `/dev/video1` `cedrus-encoder` |
| Failed units at boot | none |

The image was built on zao and the `fuji` aarch64 builder. Nothing compiled on
the SP. The probes are the Pro's prebuilt aarch64 store paths, copied with
`nix copy`.

## Encoder probes

Same commands as the Pro's acceptance script, against the booted module:

| Check | Result |
|---|---|
| Controls probe, 640×480 QP 18 / QP 36 / High, 1280×720, 1920×1080 | `CONTROLS-PASS` ×5; 62 frames each; empty LAST, EOS, START and STREAMON restart |
| Sunshine probe, 640×480 | 120 key packets, EOF |
| 600 moving frames, 10 Mbps | 182.914 fps encode; 9,963,371 B; host `verify.py` PASS, PSNR-Y min 42.919274 dB |
| 600 moving frames, 1 Mbps | 208.956 fps; 1,284,892 B; host `verify.py` PASS, PSNR-Y min 31.321423 dB |
| 30 s unpaced moving NV12 | 5,744 frames, 191.432 fps, 74.706% probe CPU |
| Kernel journal over the probes | no WARN, Oops, BUG, fault or timeout |

The byte sizes and PSNR minimums of both 600-frame streams equal the Pro's
values exactly.

## Moonlight stream

Pairing used Sunshine's authenticated PIN endpoint over loopback with a
test-only admin, as on the Pro. The panel was idle-off; it was woken through
the running compositor's `swaymsg output DSI-1 power on`.

```sh
timeout --signal=INT --kill-after=15 65 \
  env H700_DUMP=/tmp/sp-sun/usb60.h264 LD_PRELOAD=/tmp/h700-sun/client-metrics.so \
  moonlight stream 10.42.5.1 -platform fake -viewonly -width 640 -height 480 \
  -fps 60 -codec h264 -bitrate 20000 -app Desktop -keydir /tmp/sp-sun/client -quitappafter
```

| Measurement | USB Ethernet, 20000 kbps request |
|---|---:|
| Encoder chosen by `encoder=auto` | `h264_v4l2m2m` on `/dev/video1` |
| Received complete frames | 2,976 |
| Observed video duration | 63.331 s |
| Received rate (packets, not new pictures) | 46.975 fps |
| Missing frame numbers | 0 |
| Mean host-provided processing latency | 100.891 ms |
| RTT | 2–3 ms |
| VE interrupts during the session | 2,981 |

FFmpeg decoded all 2,976 received frames without errors: H.264 High, level
4.1, 640×480. Frames 1500 and 2900 show the first-boot setup screen
("SETUP HELLO") with correct colours. The screen was static, so only 31
distinct frame hashes occur; this run does not measure new-picture rate. The
Pro's frame profile (about 13.5 new KMS frames per second) is expected to
apply unchanged, because capture, kernel and encoder are the same.

## Final state

- Test pairing and admin removed: `sunshine_state.json` moved to
  `/tmp/sp-sun-test-state.json` (mode 0600) on the SP; Sunshine restarted
  and active with its shipped configuration.
- korrid, compositor and kiosk active throughout.
- zao's ED25519 public key was added to `/root/.ssh/authorized_keys` on the
  SP over the USB root console, for test access on port 2222.

**Not accepted:** Wi-Fi streaming, audio, client input, new-picture rate,
P frames and the SP v2-panel tree.
