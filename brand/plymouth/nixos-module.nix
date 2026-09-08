# The Korri boot splash: Plymouth from the initrd until the compositor paints.
#
# Ordering is the whole problem. Plymouth holds DRM master; Sway cannot take
# the KMS device until it lets go, and korri-compositor already restarts on
# EAGAIN when it loses that race. So the sequence is fixed here:
#
#   korri-boot-splash-handoff   tells the theme to assemble the wordmark and
#                               waits for the animation to finish
#   plymouth-quit               quits with --retain-splash, leaving the last
#                               frame on the framebuffer and releasing DRM
#   korri-compositor            starts only after that
#
# Nothing in this path may fail the boot: the handoff ignores errors, because
# a splash that blocks startup is worse than no splash.
{ config, lib, pkgs, ... }:
let
  cfg = config.services.korri.bootSplash;
  theme = pkgs.callPackage ./package.nix { inherit (cfg) refreshRate; };
  plymouth = config.boot.plymouth.package;

  # The theme's fall is 160 ms wind-up + 240 ms drop + 160 ms glyph snap.
  # Wait for it, then quit. Quitting early is safe — the theme's quit function
  # snaps to the finished wordmark — but it would cut the animation short.
  handoff = pkgs.writeShellApplication {
    name = "korri-boot-splash-handoff";
    runtimeInputs = [
      plymouth
      pkgs.coreutils
    ];
    text = ''
      plymouth display-message --text=korri:wordmark
      sleep ${toString cfg.animationSeconds}
    '';
  };
in
{
  options.services.korri.bootSplash = {
    enable = lib.mkEnableOption "the Korri boot splash";

    refreshRate = lib.mkOption {
      type = lib.types.ints.positive;
      default = 60;
      example = 120;
      description = ''
        Frames per second for the theme, set with Plymouth.SetRefreshRate.
        Match the panel: a rate below it repeats frames, above it discards
        them. RG353M is 60 Hz, Odin 2 Portal is 120 Hz.
      '';
    };

    animationSeconds = lib.mkOption {
      type = lib.types.float;
      default = 0.6;
      description = ''
        How long to hold before quitting Plymouth, so the wordmark finishes
        assembling. Time added to every boot.
      '';
    };

    quiet = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Hide kernel and initrd messages so the splash is the only thing on
        screen. Leave this off until the device is known to boot with the
        splash: on these handhelds the panel is the only debugging channel,
        because the serial console needs the case opened.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    boot.plymouth = {
      enable = true;
      themePackages = [ theme ];
      theme = "korri";
    };

    boot.kernelParams = lib.mkIf cfg.quiet [
      "quiet"
      "logo.nologo"
    ];
    # mkForce: the device images set consoleLogLevel themselves (the RG353M
    # uses 7), and quiet has to win or it does nothing.
    boot.consoleLogLevel = lib.mkIf cfg.quiet (lib.mkForce 0);
    boot.initrd.verbose = lib.mkIf cfg.quiet (lib.mkForce false);

    systemd.services = lib.mkMerge [
      {
        korri-boot-splash-handoff = {
          description = "Assemble the Korri wordmark before Plymouth quits";
          wantedBy = [ "multi-user.target" ];
          before = [ "plymouth-quit.service" ];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
            # A splash must never block a boot.
            ExecStart = "-${lib.getExe handoff}";
          };
        };

        # Upstream quits without --retain-splash, which blanks the screen the
        # moment Plymouth exits and leaves the panel dark until Chromium paints.
        plymouth-quit.serviceConfig.ExecStart = lib.mkForce "-${plymouth}/bin/plymouth quit --retain-splash";
      }

      # Sway must not race Plymouth for DRM master. Only where the host that
      # defines korri-compositor is present: ordering a unit that does not
      # exist would conjure an empty one.
      (lib.mkIf (config.services ? korriLinuxHost && config.services.korriLinuxHost.enable) {
        korri-compositor.after = [ "plymouth-quit.service" ];
      })
    ];
  };
}
