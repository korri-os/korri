# Product control actions every device gets: volume keys, Home+volume
# brightness, controller activity keeping the screen awake, and display idle. Moved unchanged from the RP Mini V2
# (portal.nix, 2daf0930c) so no device can ship without them.
#
# A device supplies only hardware facts: its InputPlumber data decides whether
# volume keys reach inputd, and compositor.outputName names the panel.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  host = config.services.korriLinuxHost;
  runtimeDir = "/run/user/${toString host.runtimeUid}";
  # swayidle runs each command through sh -c, which expands the panel name from
  # the unit environment. The product check compares ExecStart exactly and
  # treats unit environment as device data.
  displayIdle = pkgs.writeShellScript "korri-display-idle" ''
    exec ${pkgs.swayidle}/bin/swayidle -w \
      timeout 300 '${pkgs.sway}/bin/swaymsg output "$KORRI_DISPLAY_OUTPUT" power off' \
      resume '${pkgs.sway}/bin/swaymsg output "$KORRI_DISPLAY_OUTPUT" power on' \
      before-sleep '${pkgs.sway}/bin/swaymsg output "$KORRI_DISPLAY_OUTPUT" power off'
  '';
  volume = step: {
    command = [
      "${pkgs.wireplumber}/bin/wpctl"
      "set-volume"
      "@DEFAULT_AUDIO_SINK@"
      step
    ];
    environment.XDG_RUNTIME_DIR = runtimeDir;
  };
  # Inputd actions run as the runtime user with no capabilities, and inputd's
  # own unit keeps /sys read-only. Each brightness action may only start its
  # one root unit, which writes the root-owned sysfs brightness nodes.
  backlightStep = import ./backlight-step.nix { inherit pkgs; };
  backlightUnits = {
    korri-backlight-up = "+5";
    korri-backlight-down = "-5";
  };
  backlightUnit = step: {
    description = "Korri backlight step ${step}%";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${lib.getExe backlightStep} ${step}";
      # Root owns the brightness nodes, so writing them needs no capability.
      # ProtectSystem=strict leaves /sys writable; ProtectKernelTunables would not.
      CapabilityBoundingSet = "";
      NoNewPrivileges = true;
      PrivateNetwork = true;
      RestrictAddressFamilies = "none";
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      ProtectKernelModules = true;
      ProtectKernelLogs = true;
      ProtectControlGroups = true;
      LockPersonality = true;
      MemoryDenyWriteExecute = true;
      SystemCallArchitectures = "native";
    };
  };
  backlight = unit: {
    command = [
      "${config.systemd.package}/bin/systemctl"
      "start"
      "${unit}.service"
    ];
  };
in
{
  # InputPlumber routes volume keys to inputd's direct actions. Run wpctl as
  # the runtime user against that user's PipeWire graph; no compositor key
  # bindings are needed.
  services.korriLinuxInput.inputd.actions = {
    # Inputd observes its normalized gamepad and authenticated direct actions.
    # This command only resets the seat's idle notifier; it cannot generate
    # pointer/key events or change focus. Bounded to <= 1 Hz.
    controller-activity.command = [
      "${pkgs.sway-unwrapped}/bin/swaymsg"
      "-s"
      "/run/korri-compositor/sway-ipc.sock"
      "seat * idle_notify"
    ];
    volume-up = volume "5%+";
    volume-down = volume "5%-";
    brightness-up = backlight "korri-backlight-up";
    brightness-down = backlight "korri-backlight-down";
  };

  systemd.services.korri-backlight-up = backlightUnit backlightUnits.korri-backlight-up;
  systemd.services.korri-backlight-down = backlightUnit backlightUnits.korri-backlight-down;

  # Only the runtime user (inputd's action identity) may start, and only start,
  # the two backlight units.
  security.polkit.extraConfig = ''
    polkit.addRule(function(action, subject) {
      if (action.id == "org.freedesktop.systemd1.manage-units" &&
          subject.user == "${host.runtimeUser}" &&
          action.lookup("verb") == "start" &&
          ${builtins.toJSON (map (unit: "${unit}.service") (builtins.attrNames backlightUnits))}.indexOf(action.lookup("unit")) >= 0) {
        return polkit.Result.YES;
      }
    });
  '';

  # Keep home directories and the rest of /run/user hidden. Only wpctl's native
  # socket crosses inputd's mount namespace; no user bus is needed. The user
  # manager creates its sockets before reporting ready. Missing audio must not
  # stop controllers. If the socket itself is removed and recreated (not just
  # PipeWire restarted), restart inputd to rebind it.
  systemd.services.korri-inputd = {
    wants = [ "user@${toString host.runtimeUid}.service" ];
    after = [ "user@${toString host.runtimeUid}.service" ];
    serviceConfig = {
      ProtectHome = lib.mkForce "tmpfs";
      BindReadOnlyPaths = [ "-${runtimeDir}/pipewire-0" ];
    };
  };

  # Turn the panel off after five graphical idle minutes. Controller activity
  # resets the same native idle notifier as local compositor input.
  systemd.services.korri-display-idle = {
    description = "Korri display idle";
    wantedBy = [ "multi-user.target" ];
    requires = [ "korri-compositor.service" ];
    after = [ "korri-compositor.service" ];
    environment = {
      XDG_RUNTIME_DIR = runtimeDir;
      WAYLAND_DISPLAY = "korri-wayland";
      SWAYSOCK = "/run/korri-compositor/sway-ipc.sock";
      KORRI_DISPLAY_OUTPUT = host.compositor.outputName;
    };
    serviceConfig = {
      User = host.runtimeUser;
      Group = host.runtimeGroup;
      ExecStart = displayIdle;
      Restart = "always";
      RestartSec = 1;
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
    };
  };
}
