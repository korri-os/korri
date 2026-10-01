# Complete shared Korri product composition. Device modules import this once and
# provide hardware facts and recorded limits only.
{ korri }:
{
  lib,
  config,
  ...
}:
let
  requirements = import ./requirements.nix { inherit korri; };
  inherit (requirements) constants;
in
{
  imports = [
    ../base
    ../device-cache/nixos-module.nix
    (import ../../services/inputd/nix/korri-linux-host-core.nix { inherit korri; })
    (import ../../clients/portal/nix/nixos-module.nix {
      inherit korri;
      chromiumArgs = [ "--force-prefers-reduced-motion" ];
    })
    (import ../../services/korrid/plugin-host/nixos-module.nix { inherit korri; })
    # Lifted from the RP Mini V2 on 2026-09-30 so every device ships them.
    ./controls.nix
    ./game-library-access.nix
    ./systemd/module.nix
    ./usb-gadget.nix
    ./udev-coldplug.nix
    ./boot.nix
    ./kernel.nix
    ../../clients/portal/nix/kiosk-freezer.nix
    ../base/clock-governor.nix
  ];

  options.services.korriProduct = {
    installed = lib.mkOption {
      type = lib.types.bool;
      default = true;
      readOnly = true;
      internal = true;
      description = "Diagnostic marker for the shared Korri product composition; the gate verifies the complete resulting contract.";
    };
    sleep.states = lib.mkOption {
      type = lib.types.listOf (lib.types.enum [ "light sleep" "deep sleep" "hibernation" ]);
      default = [ ];
      description = "Sleep states verified for this device. No state means the power button and lid shut down cleanly.";
    };
  };

  config = {
    assertions = [
      {
        assertion = config.services.korriProduct.sleep.states == [ ];
        message = "Sleep states cannot be declared until the exact-session freeze and wake handler is implemented.";
      }
    ];
    # No device has verified suspend support yet. logind handles the physical
    # controls, so a device with no declared sleep state shuts down cleanly.
    services.logind.settings.Login = lib.mkIf (config.services.korriProduct.sleep.states == [ ]) {
      HandlePowerKey = "poweroff";
      HandleLidSwitch = "poweroff";
      HandleLidSwitchExternalPower = "poweroff";
      HandleLidSwitchDocked = "poweroff";
    };

    # Load-following CPU governor on every device. A device adds its GPU
    # devfreq nodes and SoC errata as hardware facts.
    services.korri.clockGovernor.enable = true;

    # networkd, where a device uses it, manages only the USB cable link, which
    # is never required for online. Its wait-online unit otherwise blocks
    # network-online.target for 120 s and fails: on the RG353M that delayed the
    # Sunshine plugin past the plugin host's deadline and left input fenced.
    # NetworkManager's wait-online still serves network-online.target.
    systemd.network.wait-online.enable = false;

    # The NixOS default cache is already in the base list. Keep one exact
    # product list rather than appending the default a second time through the
    # composed module stack.
    nix.settings.substituters = lib.mkForce constants.substituters;

    users.groups.${constants.runtimeAccount.group}.gid = constants.runtimeAccount.gid;
    users.users.${constants.runtimeAccount.user} = {
      isNormalUser = true;
      uid = constants.runtimeAccount.uid;
      group = constants.runtimeAccount.group;
      home = constants.runtimeAccount.home;
      createHome = true;
    };

    services.korriLinuxHost = {
      enable = true;
      runtimeUser = constants.runtimeAccount.user;
      runtimeUid = constants.runtimeAccount.uid;
      runtimeGroup = constants.runtimeAccount.group;
      runtimeGid = constants.runtimeAccount.gid;
      # Product discovery endpoints do not authorize publication. Ticket 05's
      # automatic first-boot identity still publishes nothing and does not join
      # federation until that separate identity behavior permits it.
      relays = lib.mkForce constants.relays;
      validation.enable = true;
      audio.enable = true;
      # The shared portal's real Wayland app id has not been accepted on a
      # device yet. Product ownership starts with no guessed or legacy id;
      # device modules cannot carry the old browser-specific exclusion.
      compositor.neverFocusAppIds = lib.mkForce constants.browserNeverFocusAppIds;
      # The product admits the reviewed Sunshine virtual-device identities.
      # The removable plugin still owns whether those devices can exist.
      compositor.allowedInputIdentifiers = [
        "48879:57005:Mouse_passthrough"
        "48879:57005:Mouse_passthrough_(absolute)"
        "48879:57005:Keyboard_passthrough"
        "48879:57005:Touch_passthrough"
        "48879:57005:Pen_passthrough"
      ];
    };

    services.korri.webSurfaceHost = {
      enable = true;
      surfaceId = lib.mkForce constants.surfaceId;
    };
    services.korri.compositor.kiosk.enable = true;
    services.korridLinuxDevice = {
      address = lib.mkForce constants.korridAddress;
      streamPrivateStateRoot = "${constants.runtimeAccount.home}/.config/sunshine";
      certificateControlDirectory = "/run/korri-certificate-control";
    };
    # Korrid owns the bounded stream-client trust effect. These optional
    # sockets remain harmless when the Sunshine plugin is not installed.
    systemd.services.korrid.environment = {
      KORRID_STREAM_CERTIFICATE_CONTROL_SOCKET = "/run/korri-certificate-control/control.sock";
      KORRID_STREAM_CERTIFICATE_CONTROL_GID = toString config.services.korriLinuxHost.serviceIdentities.korridGid;
      KORRID_STREAM_CERTIFICATE_CONTROL_PEER_UID = "0";
      KORRID_STREAM_CERTIFICATE_CONTROL_PEER_GID = "0";
      KORRID_INPUT_SEAT_CONTROL_SOCKET = "/run/korri-input-seat/control.sock";
    };

    services.korri.pluginHost = {
      enable = true;
      publishers = constants.publishers;
    };
  };
}
