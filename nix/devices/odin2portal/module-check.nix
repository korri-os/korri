# Focused evaluation of the exported Odin 2 Portal configuration.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  c = configuration.config;
  kiosk = c.systemd.services.korri-chromium-kiosk;
  compositor = c.systemd.services.korri-compositor;
  credential = "KORRID_RPC_CAPABILITY:/run/korri-portal-credentials/KORRID_RPC_CAPABILITY";
  oldBootstrapAppId = "chrome-127.0.0.1__kiosk-blank.html-Default";
  inputDataNames = map lib.getName c.services.korriLinuxInput.provider.extraDataPackages;
in
# Hardware and boot facts.
assert c.nixpkgs.hostPlatform.system == "aarch64-linux";
assert c.networking.hostName == "odin2portal";
assert c.boot.kernelPackages.kernel.version == "7.2";
assert c.boot.kernelPackages.kernel.modDirVersion == "7.2.0";
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
assert c.services.korriLinuxHost.compositor.outputName == "DSI-1";
assert c.services.korriLinuxHost.compositor.mode == "1080x1920@120Hz";
assert c.services.korriLinuxHost.compositor.renderer == "gles2";
assert c.services.korriLinuxHost.compositor.localInput.enable;
assert lib.hasInfix "output DSI-1 transform 270" c.services.korriLinuxHost.compositor.extraConfig;
assert lib.hasInfix "input type:touch map_to_output DSI-1"
  c.services.korriLinuxHost.compositor.extraConfig;
assert builtins.elem "inputplumber-ayn-sm8550-data" inputDataNames;
assert c.hardware.graphics.enable;

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

# The shared product owns the portal, its security, and game-return policy.
assert c.services.korriProduct.installed;
assert c.services.korriLinuxHost.enable;
assert c.services.korriLinuxHost.runtimeUser == "korri";
assert c.services.korriLinuxHost.runtimeUid == 1000;
assert c.services.korriLinuxHost.validation.enable;
assert c.services.korriLinuxHost.audio.enable;
assert c.services.korri.webSurfaceHost.enable;
assert c.services.korri.webSurfaceHost.surfaceId == "shift";
assert c.services.korri.compositor.kiosk.enable;
assert c.services.korri.pluginHost.enable;
assert c.services.korriLinuxHost.compositor.neverFocusAppIds == [ "chromium-browser" ];
assert !(lib.hasInfix oldBootstrapAppId c.services.korriLinuxHost.compositor.extraConfig);
assert !(configuration.options.services ? korriKiosk);
assert !(c.systemd.services ? korri-kiosk);

# Odin starts the existing private window.KorriRpc host on the compositor's
# published Wayland socket. No credential is exposed through the environment.
assert lib.hasSuffix "/bin/korri-chromium-kiosk" kiosk.serviceConfig.ExecStart;
assert kiosk.serviceConfig.Type == "notify";
assert kiosk.serviceConfig.User == "korri-portal";
assert kiosk.serviceConfig.NoNewPrivileges;
assert kiosk.serviceConfig.ProtectSystem == "strict";
assert kiosk.serviceConfig.PrivateTmp;
assert kiosk.serviceConfig.LoadCredential == [ credential ];
assert c.systemd.services.korrid.serviceConfig.LoadCredential == [ credential ];
assert !(kiosk.environment ? KORRID_RPC_CAPABILITY);
assert !(c.systemd.services.korrid.environment ? KORRID_RPC_CAPABILITY);
assert kiosk.environment.KORRID_PORTAL_ORIGIN == "http://127.0.0.1:8099";
assert kiosk.environment.KORRI_WEB_SURFACE_URL == "http://127.0.0.1:8099/?surface=shift";
assert kiosk.environment.WAYLAND_DISPLAY == compositor.environment.KORRI_WAYLAND_DISPLAY;
assert
  kiosk.serviceConfig.BindReadOnlyPaths == [
    "/run/user/1000/korri-wayland:/run/korri-portal/korri-wayland"
  ];
assert lib.elem "korri-compositor.service" kiosk.requires;
assert lib.elem "korrid.service" kiosk.requires;

# The portal composition does not depend on the optional streaming host.
assert !(c.systemd.services ? sunshine);
assert !(c.systemd.sockets ? korri-certificate-control);
assert !(lib.elem "sunshine.service" compositor.wants);
assert c.systemd.services ? korri-chromium-kiosk;

pkgs.runCommand "odin2portal-module-check" { } ''
  grep -F 'korri-portal-shell' ${kiosk.serviceConfig.ExecStart}
  grep -F -- '--ozone-platform=wayland' ${kiosk.serviceConfig.ExecStart}
  if grep -F '${oldBootstrapAppId}' ${kiosk.serviceConfig.ExecStart}; then exit 1; fi
  touch "$out"
''
