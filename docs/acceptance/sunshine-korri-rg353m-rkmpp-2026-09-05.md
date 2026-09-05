# Sunshine Korri RG353M RKMPP hardware encoding — 2026-09-05

## Scope and identities

This record covers Sunshine hardware H.264 encoding on the Anbernic RG353M through the Rockchip MPP encoder. It does not claim audio, persistent boot activation, or a physical Moonlight client session with input.

- Foundation commit: `a635d309` (Rockchip RKVENC kernel service, MPP userspace, DT overlay)
- Base Sunshine: `2025.924.154138`, reviewed upstream derivation `/nix/store/8dhfxx3xi04qvlv8ihrg4p6ycqwx6fhc-sunshine-2025.924.154138.drv`
- Reviewed FFmpeg commit: `61c50407fd429a5e2ec616e2e846c3fe3743879a`
- Build profile: `aarch64-linux-rkmpp`
- Patch `0021` SHA-256: `5fc9f90b6753791cc66107b0d8c58b78568126f71dda2fb8e739b63aae9981d6`
- Ordered sixteen-patch digest: `bfbb903bcdbab42627c9adabbd21426b0a7ddd86896fe592a49a6a2b2324d1ce`
- FFmpeg patch `0001` SHA-256: `772d3a55ea116417ee6768ce1ece4f9d9c9e6c12a8dbd541e3e1ab7c348d9ac4`
- FFmpeg patch `0002` SHA-256: `b2bcaed419e11dbb6b435f865b4fb78eca195708c14da1cbd97c29dfae50f09f`
- Tested Sunshine output: `/nix/store/lhhdw9l9dvy44l7v4ah40bmz4vrqdx2q-sunshine-korri-2025.924.154138-korri`
- Kernel: mainline Linux `6.18.2` with out-of-tree `rk_vcodec` and `rk_mpp_overlay`
- Client: `moonlight-embedded 2.7.0`, `-platform fake -viewonly`, over Ethernet

The tested Sunshine build was imported into the device store and run as a transient unit against the live Sway compositor. The persistent boot profile was not changed.

## Build path

Sunshine links against a static FFmpeg built by Sunshine's own `build-deps` recipe. The RKMPP build keeps the reviewed FFmpeg commit and applies:

- `patches/ffmpeg/0001-add-rkmpp-h264-encoder.patch`: the `h264_rkmpp` encoder and `AV_HWDEVICE_TYPE_RKMPP` hardware context lifted from the `ffmpeg-rockchip` fork at `d90e3a1c18d7929383cf88c1b3da2e2d1c966cbf`.
- `patches/ffmpeg/0002-adapt-rkmpp-to-reviewed-ffmpeg.patch`: removes `ff_encode_add_stats_side_data` and `AV_PIX_FMT_NV15`, which do not exist at the reviewed commit.

Sunshine's CBS patches are applied with `patch` because `build-deps` uses `git apply`, which no-ops outside a git checkout.

Sunshine patch `0021` adds the `rkmpp` encoder definition (H.264 only, CBR, single slice, no intra refresh) and paces its encode loop to the negotiated frame rate.

## Encoder probe

Sunshine started with `capture=kms encoder=rkmpp` and reported:

- `Trying encoder [rkmpp]`
- `Creating encoder [h264_rkmpp]`
- `Found H.264 encoder: h264_rkmpp [rkmpp]`

The probe encode produced one RKVENC interrupt. No fallback encoder was selected.

## Unpaced finding

The first build did not pace the RKMPP encode loop. During a 30 fps stream the RKVENC interrupt counter advanced at about 43 frames per second while the client requested 30. The MPP encoder returns immediately in asynchronous mode, so Sunshine's loop ran at the KMS capture rate. Patch `0021` was extended to pace the loop for the `rkmpp` encoder. After the change a 30-second stream produced 846 frames in 30.1 seconds (28.07 fps including session setup).

## Test activation and fallback finding

The complete RG353M closure `/nix/store/hqb1v91jsa4bm71gddhk97g8i8ba6iby-nixos-system-rg353m-sd-card-26.05.20251221.a653104` was activated non-persistently with `switch-to-configuration test`. The hardened `sunshine.service` started with `encoder=rkmpp` on its argv and the RKMPP-profile package as its executable.

On that first activation Sunshine logged `Couldn't find any working encoder matching [rkmpp]` without a `Trying encoder [rkmpp]` probe and silently fell back to `libx264`. The `/run/wrappers/bin/sunshine` wrapper was regenerated in the same second the unit started, during the activation's `suid-sgid-wrappers` refresh. Three manual restarts of the identical unit each selected `h264_rkmpp`. The failure did not reproduce.

A silent software fallback on this device would cost about 170% CPU and hide an RKVENC regression behind a working stream. The host module now sets `SUNSHINE_STRICT_ENCODER=1` for `encoder = "rkmpp"`, matching the existing NVENC policy, so an RKMPP probe failure fails the unit instead of streaming with x264.

The strict closure `/nix/store/rvmkwwl3zvaicxlq3zadg1kp3h61g90g-nixos-system-rg353m-sd-card-26.05.20251221.a653104` was then test-activated. Activation waited on the pre-existing 120-second `systemd-networkd-wait-online` timeout before restarting Sunshine. The hardened unit started with `SUNSHINE_STRICT_ENCODER=1`, selected `h264_rkmpp [rkmpp]`, and served a 40-second Moonlight session: 1,113 RKVENC interrupts (27.7 fps including setup, 30 fps steady), 62–70 °C, about 120% total CPU.

## Soak

A 180-second Moonlight session at 640x480, 30 fps, 4000 kbps, H.264:

- active window: 172 seconds
- RKVENC interrupts: 5,058 (29.41 fps average)
- CPU temperature: 47.8 °C idle, 70.6 °C peak, `schedutil` governor
- no encoder errors, timeouts, or IOMMU faults in the Sunshine log or kernel log
- the only kernel warning during the window was an unrelated `rtw88` Wi-Fi power-save acknowledgement failure

## CPU comparison

Per-thread CPU over an 8-second window mid-stream, same build, same client:

| Encoder | Hot thread | Encoder thread | Total |
|---|---|---|---|
| `libx264` (production) | 108% | (in hot thread) | 170% |
| `h264_rkmpp` | 106% | 1.1% (`mpp_h264e`) | 135% |

The RKVENC encoder itself costs about 1% CPU. The remaining load is Sunshine's `GetTextureSubImage` GPU-to-RAM readback and `sws_scale` BGRA-to-NV12 conversion on the capture thread. `swscale` alone measured 5.4–6.3 ms per frame single-threaded on this CPU.

For comparison, `ffmpeg -f kmsgrab ... -c:v h264_rkmpp` with DRM_PRIME frames encoded 300 frames at 32 fps using 0.43 s user time (about 7% CPU). A zero-copy DRM_PRIME capture path in Sunshine is the next step and is tracked in the backlog.

## Approval gates

- `packages.x86_64-linux.sunshine-korri` derivation is unchanged: `/nix/store/sxgpqashwhgj6gfmaha29iszzl74nmpd-sunshine-korri-2025.924.154138-korri.drv` before and after.
- `aarch64-linux-rkmpp` reuses the reviewed `aarch64-linux-software` upstream derivation; RKMPP is added only downstream.
- The profile is rejected on non-aarch64 hosts, when CUDA is also requested, or when the FFmpeg bundle, Rockchip MPP, or libdrm inputs are missing.
- `services.korriLinuxHost.sunshine.encoder = "rkmpp"` requires an RKMPP-enabled package.
- The RG353M host asserts the package carries `korriRkmppEnabled`.

## Not claimed

- Audio: PulseAudio access is denied inside the Sunshine unit; this predates RKMPP.
- Persistent activation: the closure was activated with `switch-to-configuration test` only; the persistent boot profile is still the known-good system.
- A physical client session with input, or a decode-verified stream on a real display.
