# Runtime policy ROCKNIX applies to the SM8550 that a bare NixOS boot does
# not. None of this is a kernel patch; the 53 in kernel/patches are the whole
# ROCKNIX kernel queue and are already in. These are the userspace decisions
# ROCKNIX makes at boot, ported from the same rev the kernel came from
# (f080b462) and from what legacy measured on this hardware.
#
# Each entry names its source and what breaks without it, so the next
# kernel bump can retire the ones that upstream absorbs.
{
  config,
  lib,
  pkgs,
  ...
}:

let
  ucmPackage = pkgs.callPackage ./audio { };
  ucmPath = "${ucmPackage}/share/alsa/ucm2";
in
{
  imports = [
    ./clock-governor.nix
    ./fan-control.nix
  ];

  # ROCKNIX's EXTRA_CMDLINE for SM8550 (projects/ROCKNIX/devices/SM8550/
  # options). Each flag fixes a specific thing:
  #
  #   allow_mismatched_32bit_el0   the prime cores are 64-bit-only; without
  #                                this the kernel refuses 32-bit EL0 on the
  #                                whole system and ARM32 payloads (Box64,
  #                                32-bit emulator cores) cannot run at all
  #   fw_devlink.strict=1          probe ordering for the display and DSP
  #                                supplier chains
  #   pcie_ports=compat            the WCN7850 sits on PCIe; the port driver's
  #                                native service handling misbehaves here
  #   irqaffinity=0-2              keep IRQs on the little cluster so the
  #                                prime cores stay free for the foreground
  #   cgroup.memory=nokmem,nosocket
  #                                kernel-memory accounting overhead
  #   nosoftlockup                 long GPU/DSP handshakes trip the detector
  #   usbcore.interrupt_interval_override=045e:028e:2
  #                                Xbox 360 controller polling (ROCKNIX patch
  #                                0506 adds the parameter)
  boot.kernelParams = [
    "allow_mismatched_32bit_el0"
    "fw_devlink.strict=1"
    "pcie_ports=compat"
    "irqaffinity=0-2"
    "cgroup.memory=nokmem,nosocket"
    "nosoftlockup"
    "usbcore.interrupt_interval_override=045e:028e:2"
  ];

  # Load-following governors, and the GMU stall workaround.
  #
  # The device boots with `performance` on every cluster and the GPU, which
  # pins all clocks at maximum. schedutil + simple_ondemand track demand;
  # legacy measured ~30 C cooler and far quieter under light load on this
  # SoC with no frame cost under heavy emulation.
  #
  # cpuIdleDisable is load-bearing, not tuning. GPU devfreq votes go through
  # the Adreno 740 GMU. When cpu0 is in its deep idle state (state1,
  # cpu-sleep-0-0) it misses the GMU HFI response interrupt, the vote times
  # out (`a6xx_gmu_set_oob GPU_SET timeout`), and the GPU wedges -- a hard
  # display freeze, reproduced by legacy on game launch. Holding cpu0 state1
  # disabled keeps it responsive. This mirrors ROCKNIX PR #2876; the kernel
  # fix (PR #3044, gpucc power domains + rpmhpd rail declamp) postdates our
  # pin and retires this line when the kernel is bumped past it.
  services.korri.clockGovernor = {
    enable = true;
    gpuDevfreqNodes = [ "3d00000.gpu" ];
    cpuIdleDisable = [ "cpu0/cpuidle/state1" ];
  };

  # Closed-loop fan control. The stock thermal policy maps its maximum
  # cooling state to pwm 70/255 -- quiet, and it lets gaming loads reach
  # ~90 C while the kernel believes cooling is maxed. This curve is the one
  # legacy settled on for Thor and the Portal (same fan, same hwmon name
  # `pwmfan`, same prime-core zone). Measured: the fan spins reliably down to
  # 8 % (pwm 20, ~950 RPM, near-inaudible), stalls below ~6 %, and restarts
  # from a dead stop on an 8 % command, so 0 % idle is safe and the 5 s
  # re-write doubles as the restart kick. Underclocked gaming holds ~53-58 C
  # with the fan off; from 70 C the ramp is identical to the earlier gaming
  # curve.
  services.korri.fanControl = {
    enable = true;
    hwmonName = "pwmfan";
    tempSource = {
      kind = "thermal-zone";
      zoneType = "cpu7-top-thermal";
    };
    curve = [
      {
        tempC = 58;
        pwmPercent = 8;
      }
      {
        tempC = 70;
        pwmPercent = 55;
      }
      {
        tempC = 85;
        pwmPercent = 100;
      }
    ];
    idlePwmPercent = 0;
    profileName = "thor-whisper";
  };

  # Audio. The card is `AYNOdin2`; WirePlumber needs the matching UCM
  # profile to build the speaker and headphone sinks, and it finds it
  # through ALSA_CONFIG_UCM2.
  #
  # Verified on device: the profile resolves through the real conf.d lookup,
  # `HiFi` exposes Speaker, Headphones, and DisplayPort, and enabling Speaker
  # runs the codec and aw88166 cset sequence. A 440 Hz tone through
  # hw:AYNOdin2,0 was audible from the speakers. PipeWire itself does not run
  # here yet: it carries ConditionUser=!root, and this bring-up image logs
  # in as root. Sinks appear when a real session user owns the graph, which
  # is the product layer's job (legacy: the Korri runtime user under
  # greetd). Until then, direct ALSA through hw:0 works, and the two
  # AudioReach warnings at boot (`CMD timeout for [1001021]`,
  # `soundwire dout-ports mismatch`) are benign: they occur before any
  # profile is applied and do not recur once UCM has run.
  services.pipewire = {
    enable = true;
    alsa.enable = true;
    pulse.enable = true;
    wireplumber.enable = true;
  };
  environment.sessionVariables.ALSA_CONFIG_UCM2 = ucmPath;
  systemd.user.services.pipewire.environment.ALSA_CONFIG_UCM2 = ucmPath;
  systemd.user.services.wireplumber.environment.ALSA_CONFIG_UCM2 = ucmPath;
  environment.systemPackages = [
    pkgs.alsa-utils
    ucmPackage
  ];

  # Suspend. Real S3 does not work on this SoC under the mainline kernel:
  # the ROCKNIX host and legacy both implement "fake suspend" (screen off,
  # radios down, governors to powersave, no real sleep) instead. That is a
  # product behaviour owned by Korri's session and input layers, not a
  # substrate fact, so it is deliberately not here. Until it is ported, make
  # sure nothing can attempt a real suspend, because the wake path is
  # untested and a failed resume looks like a hang.
  systemd.targets = {
    sleep.enable = false;
    suspend.enable = false;
    hibernate.enable = false;
    hybrid-sleep.enable = false;
  };
  services.logind.settings.Login = {
    HandlePowerKey = "ignore";
    HandleLidSwitch = "ignore";
  };
}
