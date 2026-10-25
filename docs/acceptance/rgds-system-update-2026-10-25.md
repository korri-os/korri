# RG DS first eMMC system update — acceptance, 2026-10-25

The first real `rgds-update` run moved the RG DS from the image installed on
2026-10-25 (source `ac3c4fa`) to source `5436bd8bc`. The device booted the new
system from eMMC. After the separate service-bundle selection, korrid,
the compositor, the portal, inputd and Sunshine were active with no failed units.

## Systems

| | Path |
|---|---|
| Before (booted, profile) | `/nix/store/45bq492lq12f6pswcw58bckb1hmnczvx-nixos-system-rgds-sd-card-26.11.20260923.4975466` |
| After (booted, current, profile) | `/nix/store/7l99ban3yp2hdcxmi35cz2vs0zj7wzyn-nixos-system-rgds-sd-card-26.11.20260923.4975466` |
| Service bundle before | `/nix/store/gjjd6p4y93v8bfc64zrcr6jmr0m45mj8-korri-bundle-0.0.0` |
| Service bundle after | `/nix/store/d6dn2vk8hazz5a79wkvypnksr0agnas2-korri-bundle-0.0.0` (korrid `/nix/store/f7b8h9r2vxakbh4id4w22a1pjlxzlgcj-korrid-0.0.0`) |

The checkout was pinned at `5436bd8bc` and clean. Access used the owner-approved
root key (zao `id_ed25519`, SHA256:C0SJMsf2V6Q6qNkanNu6/LiEX8BGOUVG85fckfMS94Q),
authorized through the USB console. The plugin host key,
SHA256:haRufzDBKR9OnvXxcwtcF4Fx/w2fyZgCzJnyBrJkJVY, was read from the console.
The SD-boot host key recorded the night before differed because that was another installation.

## What the real device found

| Run | Result | Cause and fix |
|---|---|---|
| `--status` 1 | Refused: `not on the root filesystem: /nix/store` | NixOS mounts `/nix/store` as a read-only bind of `/dev/mmcblk0p2`. Fixed in `be221982a` by comparing `st_dev` with `/`. |
| Update 1 | Stopped before any write: copying the old system back to zao refused an unsigned `korri-plugin-host` path | The locally built image closure had been collected on zao. Rebuilt `45bq492l…` on zao from `ac3c4fa`; no signature bypass. |
| Update 2 | Stopped before any write: `unsafe RG DS system path` | The generator renders the previous generation's development specialisation (`nixos-1-development`). Fixed in `5436bd8bc`; the fixtures now carry that specialisation. |
| Update 3 | `Update staged`; every staged payload checksum `OK` | |
| Reboot | SSH back about 110 s after `systemctl reboot` at 16:05:54 UTC | |

After the reboot, korrid crash-looped:
`korrid requires exactly one inherited listener for its current PID`.
The persistent service-bundle selection still pointed at the old bundle,
whose korrid predates the new unit's socket handoff. As `UPDATE.md` requires,
the RG DS bundle was then selected explicitly with `korri-bundle-select switch`.
korrid started at 16:09:54 UTC and stayed up with `NRestarts=0`.

## Verified after reboot and bundle selection

| Check | Result |
|---|---|
| Root | `/dev/mmcblk0p2`, kernel 7.2.0 |
| Boot menu | `nixos-default` (new), `nixos-1-default` (old), `nixos-1-development` (old's specialisation); `extlinux.conf.previous` kept |
| Failed units | none |
| Encoder | `/dev/mpp_service` and `/dev/dma_heap/system` are `root:video 0660`; `rk_vcodec` and `system_heap` are loaded. Sunshine logged `Found H.264 encoder: h264_rkmpp [rkmpp]` at 16:07:49. |
| Bluetooth | `hci0`, controller `44:90:BA:D9:F0:F4`, powered |
| Wi-Fi | Connected to the owner's saved network; `iw` power save `off` |
| GPU devfreq | `fde60000.gpu`, `simple_ondemand`, 200 MHz at idle |
| User state | melonDS still Enabled; the `tetris-ds-usa` library entry and the owner's `melonDS.opt` test remain |

## Not yet verified

No stream reached a client, so Sunshine stream fps and CPU are unmeasured.
Bluetooth pairing, long-run Wi-Fi stability, GPU frequency under load, a game
launch through the new korrid, and `--rollback` on hardware remain open.
The rescue SD image is still unbuilt and unwritten.
