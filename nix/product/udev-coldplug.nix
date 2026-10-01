# Boot coldplug on every device replays devices plus only the module events
# this system's udev rules name. See udev-coldplug-package.nix for the
# measurement and udev-coldplug-modules.nix for the build-time refusals.
{
  config,
  pkgs,
  ...
}:
let
  coldplug = pkgs.callPackage ./udev-coldplug-package.nix {
    udevadm = "${config.systemd.package}/bin/udevadm";
  };
  # Device fact: derived from this system's rules. Building it fails when a
  # rule needs events the narrowed coldplug does not replay.
  modules = pkgs.callPackage ./udev-coldplug-modules.nix {
    ruleDirs = [
      config.environment.etc."udev/rules.d".source
      "${config.systemd.package}/lib/udev/rules.d"
    ];
  };
in
{
  systemd.services.systemd-udev-trigger = {
    environment.KORRI_UDEV_COLDPLUG_MODULES = "${modules}";
    # systemd-udev-trigger comes from the systemd package, so this lands in a
    # drop-in. An empty ExecStart clears the package's two commands. The
    # leading "-" keeps upstream's rule: a failed trigger never fails sysinit.
    serviceConfig.ExecStart = [
      ""
      "-${coldplug}/bin/korri-udev-coldplug"
    ];
  };
}
