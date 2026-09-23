# Check the exported systems and R36T Max hardware facts and recorded limits.
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
  speakerUcm = import ./audio/check.nix { inherit pkgs; };
  deviceUcm = "${configuration.pkgs.callPackage ./audio/ucm.nix { }}/share/alsa/ucm2";
  kernelConfig = builtins.readFile ./dts/config;
  deviceFactsSource = builtins.readFile ./portal.nix;
  distributionWorkflow = builtins.readFile ../../../.github/workflows/device-images.yml;
  installerScript = pkgs.writeText "r36tmax-selected-installer" c.system.build.installBootLoader.text;

  hardwareFacts =
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
    && config.networking.networkmanager.ensureProfiles.profiles.korri.wifi.ssid == "$WIFI_SSID"
    && config.networking.networkmanager.ensureProfiles.profiles.korri.wifi-security.psk == "$WIFI_PSK"
    && config.networking.firewall.interfaces.usb0.allowedUDPPorts == [ 67 ]
    && (config.networking.firewall.interfaces.usb0.allowedTCPPorts or [ ]) == [ ]
    && lib.elem "libcomposite" config.boot.kernelModules
    && lib.elem "usb_f_acm" config.boot.kernelModules
    && lib.elem "usb_f_ncm" config.boot.kernelModules
    && config.environment.sessionVariables.ALSA_CONFIG_UCM2 == deviceUcm
    && config.systemd.globalEnvironment.ALSA_CONFIG_UCM2 == deviceUcm
    &&
      lib.hasInfix (builtins.unsafeDiscardStringContext deviceUcm)
        config.systemd.units."flight-recorder.service".text
    && config.systemd.services.usb-gadget.wantedBy == [ "multi-user.target" ]
    && config.systemd.services.flight-recorder.wantedBy == [ "sysinit.target" ];
in
assert lib.all (name: lib.elem "CONFIG_${name}=m" (lib.splitString "\n" kernelConfig)) [
  "SND_SOC_SIMPLE_AMPLIFIER"
  "NFT_CT"
  "NFT_LOG"
  "NFT_COMPAT"
  "NFT_LIMIT"
  "NFT_REJECT"
  "NETFILTER_XT_MATCH_PKTTYPE"
  "TLS"
];
assert hardwareFacts c;
assert hardwareFacts console;
assert lib.all (name: lib.elem name c.boot.kernelModules) [
  "tls"
  "tun"
  "uhid"
  "uinput"
];
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
assert !(lib.hasInfix "wifi.env" c.sdImage.populateRootCommands);
assert !(lib.hasInfix "wifi.env" console.sdImage.populateRootCommands);
assert lib.elem "panfrost" c.boot.kernelModules;
assert c.services.korriProduct.sleep.states == [ ];
assert c.services.logind.settings.Login.HandlePowerKey == "poweroff";
assert c.services.logind.settings.Login.HandleLidSwitch == "poweroff";
assert c.services.korriLinuxHost.label == "r36tmax";
assert c.services.korriLinuxHost.compositor.backend == "drm";
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
assert c.services.korriLinuxHost.compositor.allowedInputIdentifiers == [ ];
assert !(c.systemd.services ? sunshine);
assert !(c.systemd.sockets ? korri-certificate-control);
assert lib.hasInfix "Recorded limit: the normal image has no integrated H.264 encoder."
  deviceFactsSource;
assert !(lib.hasInfix "users.users" deviceFactsSource);
assert !(lib.hasInfix "users.groups" deviceFactsSource);
assert !(lib.hasInfix "runtimeUser" deviceFactsSource);
assert lib.hasInfix
  "R36T Max image distribution is on hold: package the ROCKNIX loader notices and corresponding source first."
  distributionWorkflow;
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
    test -f ${speakerUcm}
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
