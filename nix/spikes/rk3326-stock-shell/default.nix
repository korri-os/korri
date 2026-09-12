# Payload pieces for getting a persistent shell on an R36-class handheld's
# stock firmware.
#
# The device boots EmulationStation from internal eMMC with no SSH server
# and no telnet, and its USB-C port enumerates nothing, so there is no
# network or USB way in. The only writable surface the stock frontend reads
# is the SD card, which it scans for launchable scripts.
#
# The SSH server therefore has to be carried in on the card and has to be
# statically linked: the stock userland is a read-only squashfs with its own
# libc, and nothing may be installed into it.
{ nixpkgs }:

let
  system = "aarch64-linux";
  pkgs = nixpkgs.legacyPackages.${system};
in
{
  # Dropbear rather than OpenSSH: one small static binary with a built-in
  # key generator, which is what fits on a card and runs with no runtime
  # dependencies on the host filesystem.
  dropbearStatic = pkgs.pkgsStatic.dropbear;

  # The board device tree, compiled against a mainline kernel's sources
  # without building a kernel.
  boardDtb = pkgs.callPackage ./dts/dtb.nix {
    kernel = pkgs.linuxPackages_latest.kernel;
  };

  # Stock mainline plus the board device tree. No panel driver yet; see
  # dts/kernel.nix for why that is deliberate.
  boardKernel = pkgs.callPackage ./dts/kernel.nix {
    linuxPackages = pkgs.linuxPackages_latest;
  };
}
