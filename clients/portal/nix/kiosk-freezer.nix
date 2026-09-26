# Imported only at the Mini V2 product edge, with its patched systemd.
# The unit and account are the existing native kiosk and korrid artifacts.
{
  config,
  pkgs,
  lib,
  ...
}:
let
  unit = "korri-chromium-kiosk.service";
  systemd = config.systemd.package;
  # 258.2 uses stop/start verbs for the two native freezer methods. The
  # message distinguishes these from actual StopUnit/StartUnit authority.
  freezerMessage = "Authentication is required to freeze or thaw the processes of '$(unit)' unit.";
  patched = builtins.any (patch: baseNameOf (toString patch) == "stop-thaws-unit.patch") (
    systemd.patches or [ ]
  );
in
{
  # No kiosk, recovery, or unpatched systemd: no freezer authority or hooks.
  # The sibling Mini V2 systemd/module.nix supplies this exact stop-path patch.
  config = lib.mkIf (config.services.korri.compositor.kiosk.enable && patched) {
    systemd.services.korrid = {
      environment.KORRID_PORTAL_UNIT = unit;
      # Portal calls derive busctl from this same patched native artifact.
      environment.KORRID_SYSTEMCTL = lib.mkForce "${systemd}/bin/systemctl";
      serviceConfig.ExecStopPost = [
        "-${systemd}/bin/busctl --system --allow-interactive-authorization=no call org.freedesktop.systemd1 /org/freedesktop/systemd1 org.freedesktop.systemd1.Manager ThawUnit s ${unit}"
      ];
    };

    # No stop-retry companion. Patched PID 1 thaws inside its first stop
    # transaction, including restart and orderly shutdown without a live bus.
    services.dbus.packages = [
      (pkgs.writeTextDir "share/dbus-1/system.d/korri-portal-freezer.conf" ''
        <!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
          "https://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
        <busconfig>
          <policy user="korrid">
            <allow send_destination="org.freedesktop.systemd1"
                   send_interface="org.freedesktop.systemd1.Manager"
                   send_member="FreezeUnit"/>
            <allow send_destination="org.freedesktop.systemd1"
                   send_interface="org.freedesktop.systemd1.Manager"
                   send_member="ThawUnit"/>
          </policy>
        </busconfig>
      '')
    ];
    security.polkit.enable = true;
    security.polkit.extraConfig = ''
      polkit.addRule(function(action, subject) {
        if (action.id == "org.freedesktop.systemd1.manage-units" &&
            subject.user == "korrid" &&
            action.lookup("unit") == "${unit}" &&
            (action.lookup("verb") == "stop" || action.lookup("verb") == "start") &&
            action.lookup("polkit.message") == "${freezerMessage}") {
          return polkit.Result.YES;
        }
      });
    '';
  };
}
