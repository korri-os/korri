# The console kernel: ../kernel built without the product fragment, the one
# proven on the tablet. default.nix cross-builds it on x86_64 and passes it in.
# The Korri product replaces it with the product kernel (../hardware.nix).
#
# In the tabs6 repository this file also carried a nixpkgs placeholder kernel
# for builds without nix/kernel. Here the kernel always exists, so the
# placeholder path is gone; tabs6.placeholderKernel stays false and the ESP
# tool keeps its refusal.
{ pkgs, tabs6Kernel, ... }:
{
  boot.kernelPackages = pkgs.linuxPackagesFor tabs6Kernel;
  tabs6.dtb = "qcom/sm8150-samsung-gts6lwifi.dtb";
  tabs6.placeholderKernel = false;
}
