{ korri }:
{ ... }:
{
  imports = [
    ./gba-gameplay.nix
    ./network-online.nix
    (import ../../clients/portal/nix/nixos-module.nix {
      inherit korri;
      # Chromium must receive this as argv; its pinned wrapper does not read
      # CHROMIUM_FLAGS. This uses each surface's existing reduced-motion path.
      chromiumArgs = [
        "--force-prefers-reduced-motion"
        # Chromium 143's GPU process hits seccomp syscall 0x77 on this device,
        # even with angle/gles-egl. Use software drawing for this small preview
        # rather than weakening its sandbox or allowing a hot crash loop.
        "--disable-gpu"
      ];
    })
  ];
  services.korri.webSurfaceHost.enable = true;
  services.korri.compositor.kiosk.enable = true;
}
