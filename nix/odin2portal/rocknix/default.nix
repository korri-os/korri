# ROCKNIX SM8550 userspace, harvested for the AYN Odin 2 Portal.
#
# Provenance: ROCKNIX distribution rev 1178bc2238de782bf081c558c177d35bb3690021
# (branch next, 2026-09-04), the same revision as the kernel queue. Every
# file under this directory is byte-identical to its source there, with one
# exception: gamescope 0006 is rebased onto nixpkgs gamescope 3.16.18 and
# the ROCKNIX original sits next to it as .rocknix. Files ending in .rocknix
# are not applied.
#
#   gamescope/patches/       projects/ROCKNIX/packages/apps/gamescope/patches/
#   mangohud/patches/        projects/ROCKNIX/packages/apps/mangohud/patches/{common,qualcomm,SM8550}/
#   pipewire/                projects/ROCKNIX/packages/audio/pipewire/patches/SM8550/
#   alsa-lib/                projects/ROCKNIX/packages/audio/alsa-lib/patches/SM8550/
#   inputplumber/            projects/ROCKNIX/devices/SM8550/filesystem/usr/share/inputplumber/
#   hwdb/                    projects/ROCKNIX/packages/sysutils/systemd/hwdb.d/
#   gamecontrollerdb/        the AYN Odin2 line of packages/apps/gamecontrollerdb/config/gamecontrollerdb.txt
#   retroarch/               projects/ROCKNIX/packages/emulators/libretro/retroarch/retroarch-joypads/gamepads/
#   quirks/                  projects/ROCKNIX/packages/hardware/quirks/platforms/SM8550/ and
#                            packages/rocknix/sources/scripts/rocknix-fake-suspend
#
# The sway patches (static IPC socket path) are deliberately not carried.
# NixOS sets SWAYSOCK instead.
#
# What is packaged here:
#
#   pipewireQuantum  PipeWire and pipewire-pulse drop-in bodies with the
#                    SM8550 quantum floor. ROCKNIX patches the .conf.in;
#                    the same keys go through services.pipewire.extraConfig
#                    on NixOS.
#   alsaLib          alsa-lib with extended namehints on.
#   gamescope        nixpkgs gamescope with the four ROCKNIX patches.
#   mangohud         MangoHud pinned to ROCKNIX's commit (v0.8.4) with the
#                    common, Qualcomm, and SM8550 patches. See mangohud/.
#                    nixpkgs is at 0.8.2 and the patches do not apply there.
#   inputplumberData InputPlumber device match and capability map for the
#                    AYN MCU gamepad, in the share/inputplumber layout that
#                    services/inputd/nix/inputplumber-data.nix composes.
#   hwdb             udev hwdb marking *_touchscreen devices as touch only.
#   gamecontrollerdb SDL mapping line for the AYN Odin2 Gamepad.
#   retroarchJoypad  RetroArch autoconfig for the AYN Odin2 Gamepad.
#
# The quirks/ directory is reference material only. It is shell that reads
# and writes ROCKNIX's /storage settings store; nothing evaluates it. It is
# here so fake suspend, LED control, and the touch-screen toggle can be
# ported from the exact text ROCKNIX runs, not from memory.
{
  lib,
  stdenvNoCC,
  writeTextDir,
  callPackage,
  alsa-lib,
  gamescope,
}:

let
  sortedPatches =
    dir:
    map (name: dir + "/${name}") (
      lib.sort lib.lessThan (
        builtins.filter (name: lib.hasSuffix ".patch" name) (builtins.attrNames (builtins.readDir dir))
      )
    );
in
{
  # PipeWire's quantum floor. ROCKNIX raises min-quantum from 32 to 960
  # frames (20 ms at 48 kHz) in pipewire.conf and the pulse min.req,
  # min.frag, and min.quantum to 960/48000 in pipewire-pulse.conf. Lower
  # values underrun on the AudioReach DSP path.
  pipewireQuantum = {
    pipewire = {
      "context.properties" = {
        "default.clock.min-quantum" = 960;
      };
    };
    pipewire-pulse = {
      "pulse.properties" = {
        "pulse.min.req" = "960/48000";
        "pulse.min.frag" = "960/48000";
        "pulse.min.quantum" = "960/48000";
      };
    };
  };

  alsaLib = alsa-lib.overrideAttrs (previous: {
    patches = (previous.patches or [ ]) ++ sortedPatches ./alsa-lib;
  });

  gamescope = gamescope.overrideAttrs (previous: {
    patches = (previous.patches or [ ]) ++ sortedPatches ./gamescope/patches;
  });

  mangohud = callPackage ./mangohud { };

  inputplumberData = stdenvNoCC.mkDerivation {
    pname = "inputplumber-ayn-sm8550-data";
    version = "2026-09-04";
    dontUnpack = true;
    installPhase = ''
      runHook preInstall
      mkdir -p $out/share/inputplumber
      cp -a ${./inputplumber}/. $out/share/inputplumber/
      runHook postInstall
    '';
    meta.description = "ROCKNIX InputPlumber device match and capability map for the AYN MCU gamepad";
  };

  hwdb = writeTextDir "lib/udev/hwdb.d/61-thor-ft5x06.hwdb" (builtins.readFile ./hwdb/61-thor-ft5x06.hwdb);

  gamecontrollerdb = ./gamecontrollerdb/ayn-odin2.txt;

  retroarchJoypad = ./retroarch + "/AYN Odin2 Gamepad.cfg";
}
