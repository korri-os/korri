# OnePlus 6 hardware facts and recorded limits for the shared Korri product.
# Everything else about the phone (kernel, firmware, modem, Wi-Fi, boot.img)
# comes from the Mobile NixOS fork (default.nix).
{
  lib,
  pkgs,
  op6Kernel,
  ...
}:
let
  # Stage-1 of Mobile NixOS builds its own gadget, g1 (NCM ncm.usb0 and ACM),
  # and leaves it on the controller at switch_root; its stage-2 gadget tears
  # it down first. Korri's gadget replaces the stage-2 one, so it does the
  # same teardown before it binds. Without it the controller is busy, and a
  # second ncm.usb0 would be named usb1, which no network config matches.
  # The steps are those of the fork's modules/usb-gadget.nix.
  dropStageOneGadget = pkgs.writeShellScript "op6-drop-stage-1-gadget" ''
    gadget=/sys/kernel/config/usb_gadget/g1
    [ -d "$gadget" ] || exit 0
    echo "" > "$gadget/UDC" 2>/dev/null || true
    ${pkgs.findutils}/bin/find "$gadget/configs" -mindepth 2 -maxdepth 2 -type l -delete
    for d in "$gadget"/configs/*/strings/* "$gadget"/configs/* \
             "$gadget"/functions/* "$gadget"/strings/*; do
      [ -d "$d" ] && rmdir "$d"
    done
    rmdir "$gadget"
  '';
in
{
  networking.hostName = "op6";

  # The modem, Wi-Fi and GPU firmware is unfree.
  nixpkgs.config.allowUnfree = true;

  # The kernel the phone already boots from its boot.img (default.nix).
  mobile.boot.stage-1.kernel.package = lib.mkForce op6Kernel;
  boot.kernelPackages = lib.mkForce (pkgs.linuxPackagesFor op6Kernel);
  services.korriProduct.kernel.overrideReason = ''
    The OnePlus 6 runs the Mobile NixOS fork's sdm845-linus kernel, 7.4.0-rc3,
    the kernel the phone already boots (phoneputer ea5810e). No 7.2 kernel
    for the SDM845 board has been built or tried. Keeping this kernel keeps
    the boot.img in boot_a/boot_b unchanged.
  '';

  # One gadget owns the controller: Korri's (NCM + ACM root console).
  mobile.usb.stage-2.enable = false;
  systemd.services.usb-gadget.serviceConfig.ExecStartPre = [ "${dropStageOneGadget}" ];
  services.korriProduct.usbGadget = {
    name = "op6";
    product = "OnePlus 6 Korri";
    # The next free /24 after the Thor (10.42.8). "OP6" in ASCII after the
    # shared 02:52 prefix.
    address = "10.42.9.1";
    hostMac = "02:52:4f:50:36:01";
    deviceMac = "02:52:4f:50:36:02";
    udc = "a600000.usb";
  };

  # No sleep state is verified with Korri's session freeze.
  services.korriProduct.sleep.states = [ ];

  services.korriLinuxHost = {
    label = "op6";
    compositor = {
      backend = "drm";
      # The msm KMS device (DPU at ae01000), by path, as on the phone.
      drmDevice = "/dev/dri/by-path/platform-ae01000.display-controller-card";
      renderDevice = "/dev/dri/renderD128";
      mainDisplay = "DSI-1";
      displays = [
        {
          # Samsung sofef00 AMOLED, one DSI link (card0-DSI-1 on the phone).
          name = "DSI-1";
          mode = "1080x2280@60Hz";
          x = 0;
          y = 0;
          touch = null;
        }
      ];
      renderer = "gles2";
      # Touch (Synaptics S3706B) reaches the shell through the compositor.
      localInput.enable = true;
    };
  };
}
