{ pkgs }:
# Use the same x86_64 -> aarch64 toolchain selected by the Odin kernel area.
# This avoids native-builder disk pressure without changing browser sources.
import ./package.nix {
  pkgs = pkgs.pkgsCross.aarch64-multiplatform;
}
