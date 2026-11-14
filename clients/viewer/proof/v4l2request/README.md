# Handheld `v4l2request` path — build proof, 2026-11-13

No device was used. The RG353M and the RG35XX Pro were not contacted. The
bench proof is a later step with its own rules.

Code commit: `1ee6f235309a4ebda0049a2f6b4914584ecdaeb7`, on `origin/main`
`69c01807e`. Before the rebase it was `0439610ad` on `1ff54c502`. All five
derivations below have the same `.drv` path before and after the rebase.
`carried-source.txt` was made at `b3cd99638`, with this commit's script.
Both change only the comparison script; the compared files are the same.

Built on zao. The aarch64 derivations built on the aarch64 builder fuji,
through `/etc/nix/machines`. Nothing was built on a device.

| Output | Store path |
| --- | --- |
| `packages.aarch64-linux.korri-viewer` | `/nix/store/q1qp2yalik7jsdsdh2cr0xz8vmw02iwa-korri-viewer-0.1.0` |
| Its FFmpeg (patched), `lib` output | `/nix/store/ipfhgpk0d14r3jf8i7fwyzrppg6wv6x3-ffmpeg-9.0.1-lib` |
| `checks.x86_64-linux.korri-viewer-aarch64` | `/nix/store/jp2sdq1ddmchgz1m95wjh2129hf1hdvs-korri-viewer-aarch64-check` |
| `packages.x86_64-linux.korri-viewer` | `/nix/store/jnyfi5nkshw1963kn99riarq9riqjnyf-korri-viewer-0.1.0` |
| `checks.x86_64-linux.korri-viewer-picture` | `/nix/store/bvzsyizwnxqijpn7cgnpsn6pirpa37w3-korri-viewer-picture-check` |
| `checks.x86_64-linux.korri-viewer-io` | `/nix/store/6306a58pv8r4r2mf5ahc7j6n9fpawh9w-korri-viewer-io-check` |

## Checks

| Check | Result |
| --- | --- |
| aarch64 package build, including its Rust tests on fuji | Passed: 28 tests, 3 ignored, as before plus the handheld test. `options::tests::handheld_arguments_from_korrid` ran (the `v4l2request` cfg was on). |
| FFmpeg build log | The patch applied with no rejected hunk. Configure enabled `h264_v4l2request` and `hevc_v4l2request`. |
| `cargo clippy --all-targets -- -D warnings` for aarch64 (cfg on), in a one-off derivation on fuji | Passed. |
| `checks.x86_64-linux.korri-viewer-aarch64` (QEMU user emulation) | Passed. Outputs are in `aarch64-check/`. `--help` started with `LD_BIND_NOW=1`. The loader resolved `libavcodec.so.62` to the patched FFmpeg and found every library. That FFmpeg lists `v4l2request`. korrid's handheld arguments parsed and stopped at the missing pin; `-platform v4l2m2m` was refused. |
| `compare.sh` against `korri-plugins` `2094c1e77471` | `carried-source.txt`. `video/v4l2m2m.c`: 1016 lines removed, 1 added (the changed `draw_frame` line). `video/video.h` and `ffmpeg/0001-v4l2-request.patch` identical. The 15 sound and input files have the same hashes as the slice 2a record. |
| x86_64 dev shell: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` under Xvfb | Passed: 27 tests, 3 ignored. The x86_64 Nix build ran the same 27. |
| `checks.x86_64-linux.korri-viewer-picture`, `korri-viewer-io` | Passed. |
| `nixfmt --check` on the changed Nix files; `git diff --check` | Passed, except the FFmpeg patch, which keeps the plugin's trailing whitespace byte for byte. |

## The x86_64 package is unchanged

Compared with the base `1ff54c502`. Its viewer derivations are the same as
`origin/main`'s.

| Comparison | Result |
| --- | --- |
| Package derivation | Same input derivations (same FFmpeg and libraries) and same environment. Only `src` differs. |
| Picture and I/O check derivations | Same input derivations. The environment differs only in the source path. |
| Runtime closure | Identical apart from the package itself (290 paths). |
| `NEEDED`, `RUNPATH`, dynamic symbols | Identical (22 `NEEDED`). |
| Installed file tree, mapping file, licence | Identical. `bin/korri-viewer` grew by 144 bytes, from the Rust changes. |
| Strings of the handheld path in the binary | None (`setup: decoder=v4l2request`, `MOONLIGHT_AUTO_WINDOW_RESIZE`, `picture_init_request`, `viewer_input_init_evdev`). |
| 11 command lines, old and new binary (`--help`, `-input`, `-platform v4l2request`, `v4l2m2m`, PC flags, bad values, `pair`) | Identical output and exit status. |

## Not proven

Decode, presentation, frame timing, input, sound and stream stability on
RK3566 or H700 hardware. QEMU ran only argument parsing and FFmpeg's
`-hwaccels`; no V4L2 device exists there.
