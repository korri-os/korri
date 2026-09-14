# R36T Max speaker routing

The source declares the external amplifier and carries the native UCM
headphone guard. Host checks pass. **Driver probe, audible output, speaker-off
behavior and headphone transitions are not accepted.** No sound server is
enabled by this module; raw ALSA playback does not apply UCM policy.

## Source and contract

| Part | Grounding |
|---|---|
| Amplifier and both channels | ROCKNIX/distribution `697d112a64e442e79e3d27b064351d917657301a`, `projects/ROCKNIX/devices/RK3326/linux/dts/rockchip/rk3326-gameconsole-eeclone.dts`:292–330,896–900. |
| Enable GPIO | The same source and saved vendor `rg42t.dts` (SHA256 `f313a0e63b0804f204158fa45a1b7a73cbabdb2c171f87ae18dc55ca7b38b8cc`): GPIO3 PA7, active-high. Vendor `spk-con-gpio` is translated to the native amplifier binding, not copied as a codec property. |
| Native DT properties | Linux 6.12.63 `Documentation/devicetree/bindings/sound/simple-audio-amplifier.yaml` and `simple-card.yaml`. |
| Headphone guard | The unchanged patch from the same ROCKNIX revision, `projects/ROCKNIX/packages/audio/alsa-ucm-conf/patches/RK3326/0001_fix_speaker_when_booting_with_headphones.patch`. |
| UCM directory selection | ALSA 1.2.14 `src/ucm/utils.c`:337–350 reads `ALSA_CONFIG_UCM2`. Its `ucm2/conf.d/simple-card/rk817_int.conf` resolves to `Rockchip/rk817-sound/rk817-sound.conf`. |

The existing `rk817_int` card keeps I2S1, RK817, 256× MCLK, PC6 headphone
detection and the mic route. The amp uses GPIO-mode, pull-none pinctrl.
`HPOL/HPOR` feed `Speaker Amp INL/INR`; both `OUTL/OUTR` feed `Internal
Speakers`. This follows ROCKNIX's complete software graph, not a claim of
physical stereo speakers. An unconnected output can keep the shared amp
`DRV` powered (Linux `soc-dapm.c`:2813–2850).

`simple-audio-card,pin-switches = "Internal Speakers"` produces `Internal
Speakers Switch` (`soc-core.c`:3054–3098). Existing UCM detects that control
and selects **HP**, not the RK817 internal SPK/Class-D path. The ROCKNIX patch
explicitly turns the speaker switch off when Headphones is enabled, including
boot with the jack inserted. Jack detection alone gates only `Headphones`;
it is not an inverted speaker-mute control.

**Binding correction to the earlier integration audit:** Linux 6.12.63 does
allow optional `VCC-supply`. It does not require it. No supply is added because
no controllable board rail is established, not because the schema forbids it.
The amplifier driver declares a VCC DAPM supply; inspect probe/supply warnings
on the acceptance boot rather than inventing a regulator. No gain, battery,
charger, GPIO hog or manual GPIO operation is introduced.

## Device-scoped UCM

`ucm.nix` patches only the data package. `default.nix` selects its complete
native tree through `environment.sessionVariables` and
`systemd.globalEnvironment`, the same NixOS seams used by `hardware.alsa` for
its native device variables. Only the R36T Max hardware module imports it.
The full tree preserves native includes and discovery; the host check requires
that only `Rockchip/rk817-sound/HiFi.conf` differs. There is no new Korri schema,
`alsa-lib` overlay, duplicated HiFi configuration or global library rebuild.

Cost: all services in this device configuration receive the directory variable.
Deployment may restart services whose generated environment changed. Already
running processes need restart or a new login to receive it. Processes that
bypass UCM, discard this environment or override the directory do not get the
guard. The module does not provide session switching policy by itself.

## Host checks and module dependency

The existing `dts/game-buttons-check.nix` compiles only this board DTB with
host tools. It now checks the speaker and retains 20 malformed-speaker DTB
cases, the 14 button cases, and the radio/eMMC checks. `audio/check.nix` uses
the real libasound parser, checks the native policy and rejects four UCM
mutations. It opens no sound card and does not evaluate `ControlExists` or
execute mixer sequences. Both are included by `module-check.nix` in the
existing `r36tmax-check` task. The focused command in [dts/README.md](../dts/README.md)
can select `audio/check.nix` instead for the UCM-only check.

The retained configuration has `CONFIG_SND_SOC_SIMPLE_AMPLIFIER=m` and the
simple-card core built in. The host inspected the recorded 6.12.63 module tree:
`snd-soc-simple-amplifier.ko` exists, has the OF `simple-audio-amplifier` alias,
and resolves through `modprobe --show-depends` without additional module
dependencies. This is a read-only resolution check, not a load. Normal platform
OF autoload is the intended path; no forced module list is added. A missing
auxiliary component defers the **whole card**, including headphones
(`soc-core.c`:1750–1761). Verify inclusion, autoload and card registration again
in the actual new generation; the host checks do not build that kernel.

## Physical acceptance remains gated

- Confirm owner presence, current quiet idle and normal enclosure/battery heat.
  Keep the battery/charger child absent. Use a separately authorized build and
  deployment; this host-only slice performs neither.
- Verify the selected DTB, matching amp module, successful component/card probe,
  both DAPM channels, UCM directory and actual controls. Keep the endpoint off
  before route or level changes; select HP. Control competing UCM/session
  processes and verify an uncontended, closed PCM.
- The saved survey had Master at 100%. Read both channels' real dB range, set
  and verify the lowest reported level before enabling the endpoint. There is
  no measured safe amplifier gain or speaker power limit.
- Use only an independently attenuated, finite, ramped, zero-DC test signal
  after confirming PCM constraints. Arm a local time limit and cleanup that
  closes PCM and switches the endpoint off even if the control link fails.
  Silence means stop and investigate, not switch to SPK or raise gain.
- Verify the endpoint and DAPM settle off after the actual powerdown delay
  (Linux's default is 5000 ms and RK817 opts in), then check buzz and heat.
  Kernel state does not measure the electrical enable line.
- With headphones available, check insertion/removal, boot inserted, speaker
  mute during headphone playback, and cleanup. Preserve panel, Wi-Fi, all 17
  buttons and quiet idle on that boot. Registration alone is not acceptance.
