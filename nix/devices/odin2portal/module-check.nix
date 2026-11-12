# Focused evaluation of the exported Odin 2 Portal configuration.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  c = configuration.config;
  shell = c.systemd.services.korri-shell;
  compositor = c.systemd.services.korri-compositor;
  credential = "KORRID_RPC_CAPABILITY:/run/korri-shell-credentials/KORRID_RPC_CAPABILITY";
  inputDataNames = map lib.getName c.services.korriLinuxInput.provider.extraDataPackages;
in
# Hardware and boot facts.
assert c.nixpkgs.hostPlatform.system == "aarch64-linux";
assert c.networking.hostName == "odin2portal";
assert c.boot.kernelPackages.kernel.version == "7.2";
assert c.boot.kernelPackages.kernel.modDirVersion == "7.2.0";
assert lib.all (kernel: kernel.features.efiBootStub && kernel.config.CONFIG_EFI_STUB == "y") [
  c.boot.kernelPackages.kernel
  c.specialisation.linux-7_0_2-rescue.configuration.boot.kernelPackages.kernel
];
assert c.hardware.deviceTree.name == "qcom/qcs8550-ayn-odin2portal.dtb";
assert c.boot.loader.systemd-boot.enable;
assert !c.boot.loader.grub.enable;
assert !c.boot.loader.efi.canTouchEfiVariables;
assert c.fileSystems."/boot".device == "/dev/disk/by-label/ODIN2P_ESP";
assert c.sdImage.firmwarePartitionName == "ODIN2P_ESP";
assert c.sdImage.rootVolumeLabel == "ODIN2P_ROOT";
assert c.services.korriLinuxHost.compositor.backend == "drm";
assert c.services.korriLinuxHost.compositor.drmDevice == "/dev/dri/card0";
assert c.services.korriLinuxHost.compositor.renderDevice == "/dev/dri/renderD128";
assert c.services.korriLinuxHost.compositor.mainDisplay == "DSI-1";
assert (builtins.head c.services.korriLinuxHost.compositor.displays).mode == "1080x1920@120Hz";
assert c.services.korriLinuxHost.compositor.renderer == "gles2";
assert c.services.korriLinuxHost.compositor.localInput.enable;
assert lib.hasInfix "output DSI-1 transform 270" c.services.korriLinuxHost.compositor.extraConfig;
assert lib.hasInfix "input type:touch map_to_output DSI-1"
  c.services.korriLinuxHost.compositor.extraConfig;
assert builtins.elem "inputplumber-ayn-sm8550-data" inputDataNames;
assert c.hardware.graphics.enable;
# The firewall and the plugin host's port rules use iptables-nft. Without these
# the firewall fails and the SSH and Sunshine plugins cannot open their ports.
assert lib.all c.boot.kernelPackages.kernel.config.isEnabled [
  "NFT_COMPAT"
  "NFT_CT"
  "NFT_LOG"
  "NETFILTER_XT_MATCH_COMMENT"
  "NETFILTER_XT_MATCH_PKTTYPE"
];

# Explicit hardware limits and safety policy stay device-owned.
assert c.services.korri.clockGovernor.enable;
assert c.services.korri.clockGovernor.gpuDevfreqNodes == [ "3d00000.gpu" ];
assert c.services.korri.clockGovernor.cpuIdleDisable == [ "cpu0/cpuidle/state1" ];
assert c.services.korri.fanControl.enable;
assert c.services.korri.fanControl.hwmonName == "pwmfan";
assert c.services.korri.fanControl.tempSource.zoneType == "cpu7-top-thermal";
assert !c.systemd.targets.sleep.enable;
assert !c.systemd.targets.suspend.enable;
assert !c.systemd.targets.hibernate.enable;
assert !c.systemd.targets.hybrid-sleep.enable;
assert c.boot.initrd.compressor == "gzip";
assert !c.boot.initrd.includeDefaultModules;
assert c.hardware.firmwareCompression == "none";

# The shared product owns the shell, its security, and game-return policy.
assert c.services.korriProduct.installed;
assert c.services.korriLinuxHost.enable;
assert c.services.korriLinuxHost.runtimeUser == "korri";
assert c.services.korriLinuxHost.runtimeUid == 1000;
assert !c.services.korriLinuxHost.validation.enable;
assert c.services.korriLinuxHost.audio.enable;
assert c.services.korri.shell.enable;
assert c.services.korri.shell.startSurface == "velvet";
assert c.services.korri.pluginHost.enable;
assert c.systemd.services.korrid.environment.KORRID_SHELL_UNIT == "korri-shell.service";
assert c.systemd.services.korrid.environment.KORRID_SHELL_FREEZER_ENABLED == "true";

# Odin starts the native shell on the compositor's published Wayland socket.
# No credential is exposed through the environment.
assert lib.hasInfix "/bin/korri-shell " shell.serviceConfig.ExecStart;
assert lib.hasInfix " --installed " shell.serviceConfig.ExecStart;
assert shell.serviceConfig.User == "korri-shell";
assert shell.serviceConfig.NoNewPrivileges;
assert shell.serviceConfig.ProtectSystem == "strict";
assert shell.serviceConfig.PrivateTmp;
assert shell.serviceConfig.PrivateDevices;
assert shell.serviceConfig.LoadCredential == [ credential ];
assert c.systemd.services.korrid.serviceConfig.LoadCredential == [ credential ];
assert !(shell.environment ? KORRID_RPC_CAPABILITY);
assert !(c.systemd.services.korrid.environment ? KORRID_RPC_CAPABILITY);
assert shell.environment.KORRID_SHELL_ORIGIN == c.systemd.services.korrid.environment.KORRID_SHELL_ORIGIN;
assert shell.environment.WAYLAND_DISPLAY == compositor.environment.KORRI_WAYLAND_DISPLAY;
assert
  shell.serviceConfig.BindReadOnlyPaths == [
    "/run/user/1000/korri-wayland:/run/korri-shell/korri-wayland"
  ];
assert lib.elem "korri-compositor.service" shell.requires;
assert lib.elem "korrid.service" shell.requires;

assert c.services.korriProduct.sleep.states == [ ];
assert c.services.logind.settings.Login.HandlePowerKey == "poweroff";
assert c.services.logind.settings.Login.HandleLidSwitch == "poweroff";

# The shell composition does not depend on the optional streaming host.
assert !(c.systemd.services ? sunshine);
assert !(c.systemd.sockets ? korri-certificate-control);
assert !(lib.elem "sunshine.service" compositor.wants);
assert c.systemd.services ? korri-shell;

pkgs.runCommand "odin2portal-module-check" { } ''
  touch "$out"
''
