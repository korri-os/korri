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

    # Plymouth forces the details plugin when a serial console is on the
    # kernel command line (main.c:1188, has_serial_consoles ->
    # should_force_details), and then no theme ever draws. Both handhelds
    # keep a serial console for debugging, so opt out of that rule; serial
    # still receives kernel output, it just no longer suppresses the splash.
    boot.kernelParams = [
      "plymouth.ignore-serial-consoles"
    ]
    ++ lib.optionals cfg.quiet [
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
        #
        # plymouth-quit.service comes from the plymouth package, so this lands
        # in a drop-in, and ExecStart in a drop-in appends. The empty first
        # entry emits a bare `ExecStart=` that clears the package's command;
        # without it systemd runs both, the plain quit wins, and the splash is
        # gone before the retained one is reached.
        plymouth-quit.serviceConfig.ExecStart = [
          ""
          "-${plymouth}/bin/plymouth quit --retain-splash"
        ];
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
