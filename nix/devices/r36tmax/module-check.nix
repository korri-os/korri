# Check the two exported systems, never a second fixture model of the image.
{
  pkgs,
  configuration,
  consoleConfiguration,
  diagnosticConfiguration,
  mainlineConfiguration,
}:
let
  inherit (pkgs) lib;
  c = configuration.config;
  console = consoleConfiguration.config;
  sanity = pkgs.callPackage ./session-sanity.nix { };
  gameButtons = import ./dts/game-buttons-check.nix { inherit pkgs; };
  common =
    config:
    lib.all (a: a.assertion) config.assertions
    && builtins.isString config.system.build.toplevel.drvPath
    && config.nixpkgs.hostPlatform.system == "aarch64-linux"
    && config.hardware.deviceTree.name == "rockchip/rk3326-aislpc-r36t-max.dtb"
    && config.hardware.deviceTree.overlays == [ ]
    && lib.hasInfix "panel-generic-dsi.c" config.boot.kernelPackages.kernel.postPatch
    && !config.hardware.enableAllHardware
    && !config.boot.initrd.allowMissingModules
    && config.boot.initrd.availableKernelModules == [ ]
    && config.boot.initrd.kernelModules == [ ]
    && config.boot.initrd.compressor == "lzop"
    && config.boot.loader.generic-extlinux-compatible.enable
    && !config.boot.loader.grub.enable
    && config.fileSystems."/".device == "/dev/disk/by-label/NIXOS_R36TMAX"
    && lib.all (fs: !(lib.hasPrefix "/dev/mmcblk" fs.device)) (lib.attrValues config.fileSystems)
    && config.swapDevices == [ ]
    && config.sdImage.firmwarePartitionOffset == 16
    && config.services.getty.autologinUser == "root"
    && !config.services.openssh.enable
    && !config.services.openssh.openFirewall
    && !config.services.openssh.settings.PasswordAuthentication
    && !config.services.openssh.settings.KbdInteractiveAuthentication
    && config.users.users.root.openssh.authorizedKeys.keys == [ ]
    && config.nix.settings.max-jobs == 0
    && !config.nix.distributedBuilds
    && config.nix.settings.builders == ""
    && !config.nix.settings.fallback
    && config.nix.settings.require-sigs
    && config.nix.settings.always-allow-substitutes
    && config.system.stateVersion == "25.11"
    && config.networking.networkmanager.enable
    && config.networking.networkmanager.ensureProfiles.profiles.korri.wifi.ssid == "$WIFI_SSID"
    && config.networking.networkmanager.ensureProfiles.profiles.korri.wifi-security.psk == "$WIFI_PSK"
    && config.networking.firewall.interfaces.usb0.allowedUDPPorts == [ 67 ]
    && (config.networking.firewall.interfaces.usb0.allowedTCPPorts or [ ]) == [ ]
    && lib.elem "libcomposite" config.boot.kernelModules
    && lib.elem "usb_f_acm" config.boot.kernelModules
    && lib.elem "usb_f_ncm" config.boot.kernelModules
    && config.systemd.services.usb-gadget.wantedBy == [ "multi-user.target" ]
    && config.systemd.services.flight-recorder.wantedBy == [ "sysinit.target" ];
  kiosk = c.systemd.services.korri-chromium-kiosk;
  credential = "KORRID_RPC_CAPABILITY:/run/korri-portal-credentials/KORRID_RPC_CAPABILITY";
  kernelConfig = builtins.readFile ./dts/config;
  installerScript = pkgs.writeText "r36tmax-selected-installer" c.system.build.installBootLoader.text;
in
assert lib.all (name: lib.elem "CONFIG_${name}=m" (lib.splitString "\n" kernelConfig)) [
  "NFT_CT"
  "NFT_LOG"
  "NFT_COMPAT"
  "NFT_LIMIT"
  "NFT_REJECT"
  "NETFILTER_XT_MATCH_PKTTYPE"
];
assert !(lib.hasInfix "wifi.env" c.sdImage.populateRootCommands);
assert !(lib.hasInfix "wifi.env" console.sdImage.populateRootCommands);
# The running korrid consumes a systemd credential and KORRID_PORTAL_ORIGIN.
# A launcher expecting the retired brain.json producer is not compatible.
assert !(c.systemd.services ? korri-kiosk);
assert c.systemd.services.korrid.environment.KORRID_PORTAL_ORIGIN == "http://127.0.0.1:8099";
assert common c;
assert common console;
assert lib.all (a: a.assertion) diagnosticConfiguration.config.assertions;
assert lib.all (a: a.assertion) mainlineConfiguration.config.assertions;
assert diagnosticConfiguration.config.services.openssh.enable;
assert !diagnosticConfiguration.config.services.openssh.settings.PasswordAuthentication;
assert !diagnosticConfiguration.config.services.openssh.settings.KbdInteractiveAuthentication;
assert
  diagnosticConfiguration.config.services.openssh.settings.PermitRootLogin == "prohibit-password";
assert diagnosticConfiguration.config.users.users.root.openssh.authorizedKeys.keys == [ ];
assert mainlineConfiguration.config.sdImage.populateFirmwareCommands == ":";
assert builtins.length c.boot.extraModulePackages == 1;
assert lib.getName (builtins.head c.boot.extraModulePackages) == "rk915";
assert c.boot.kernelPackages.kernel.drvPath == console.boot.kernelPackages.kernel.drvPath;
assert lib.all (name: !(builtins.hasAttr name console.systemd.services)) [
  "korri-compositor"
  "korrid"
  "korri-kiosk"
  "korri-chromium-kiosk"
  "korri-portal-credentials"
  "sunshine"
  "korri-inputd"
  "inputplumber"
];
assert !(console.services.static-web-server.enable);
assert !console.services.nginx.enable;
assert c.services.korriLinuxHost.enable;
assert c.services.korridLinuxDevice.enable;
assert c.services.korri.webSurfaceHost.enable;
assert c.services.korri.webSurfaceHost.surfaceId == "pico";
assert c.services.korri.compositor.kiosk.enable;
assert !c.services.korridLinuxDevice.browser.enable;
assert !(c.systemd.services.korrid.environment ? KORRID_BROWSER_INFO_PATH);
assert !c.services.static-web-server.enable;
assert c.services.nginx.enable;
assert builtins.length c.services.nginx.virtualHosts.korri-portal.listen == 1;
assert lib.all (
  listener: listener.addr == "127.0.0.1" && listener.port == 8099
) c.services.nginx.virtualHosts.korri-portal.listen;
assert c.systemd.services.korrid.serviceConfig.LoadCredential == [ credential ];
assert kiosk.serviceConfig.LoadCredential == [ credential ];
assert lib.elem "korri-portal-credentials.service" c.systemd.services.korrid.requires;
assert lib.elem "korri-portal-credentials.service" kiosk.requires;
assert kiosk.environment.KORRID_PORTAL_ORIGIN == "http://127.0.0.1:8099";
assert
  c.services.korriLinuxHost.compositor.drmDevice
  == "/dev/dri/by-path/platform-display-subsystem-card";
assert
  c.services.korriLinuxHost.compositor.renderDevice
  == "/dev/dri/by-path/platform-ff400000.gpu-render";
assert c.services.korriLinuxHost.compositor.outputName == "DSI-1";
assert c.services.korriLinuxHost.compositor.mode == "720x720@61Hz";
assert c.services.korriLinuxHost.compositor.renderer == "gles2";
assert !c.services.korriLinuxHost.compositor.localInput.enable;
assert !c.services.korriLinuxHost.compositor.remoteInput.enable;
assert !c.services.korriLinuxHost.audio.enable;
assert !c.services.korriLinuxHost.validation.enable;
assert lib.elem "panfrost" c.boot.kernelModules;
assert c.services.sunshine.enable;
assert c.services.korriLinuxInput.inputd.enable;
assert c.services.korriLinuxInput.provider.enable;
assert !c.services.korriLinuxHost.sunshine.openFirewall;
assert c.services.korriLinuxHost.firewallInterfaces == [ ];
assert c.services.korriLinuxHost.ownerBindingFile == null;
assert c.services.korriLinuxHost.nativePeers == [ ];
assert c.services.korriLinuxHost.relays == [ "ws://127.0.0.1:9" ];
assert kiosk.serviceConfig.NoNewPrivileges;
assert kiosk.serviceConfig.PrivateTmp;
assert kiosk.serviceConfig.ProtectSystem == "strict";
assert kiosk.serviceConfig.LimitCORE == 0;
assert kiosk.serviceConfig.User != "root";
assert lib.elem "korrid.service" kiosk.requires;
assert lib.elem "korri-compositor.service" kiosk.after;
assert lib.hasSuffix "/bin/korri-chromium-kiosk" kiosk.serviceConfig.ExecStart;
assert !(lib.hasInfix "--no-sandbox" kiosk.serviceConfig.ExecStart);
assert lib.elem "korrid" (map lib.getName c.environment.systemPackages);
pkgs.runCommand "r36tmax-module-check"
  {
    nativeBuildInputs = [
      pkgs.bash
      pkgs.shellcheck
      pkgs.python3
    ];
  }
  ''
    test -f ${gameButtons}/rockchip/rk3326-aislpc-r36t-max.dtb
    cp ${./boot-media-check.py} boot-media-check.py
    cp ${./check-boot-media.sh} check-boot-media.sh
    mkdir payload
    cp ${./payload/boot-media.sh} payload/boot-media.sh
    python3 boot-media-check.py
    bash -n ${./session-sanity.sh}
    shellcheck --shell=bash ${./session-sanity.sh}
    # Exercise the real packaged command's argument guard without reading this
    # build machine as though it were the handheld.
    status=0
    ${lib.getExe sanity} unexpected >usage 2>&1 || status=$?
    test "$status" = 2
    grep -q 'at most 47 seconds' usage
    # Refuse the real generation-install command rather than updating /boot
    # while the loader continues to read the old FAT entry.
    status=0
    bash ${installerScript} >install-error 2>&1 || status=$?
    test "$status" = 1
    grep -q 'complete SD image rewrite' install-error
    touch "$out"
  ''
