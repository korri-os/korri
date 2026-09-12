# Compile the R36T Max board device tree, and nothing else.
#
# A device tree blob does not need a kernel build to produce it. Splitting it
# out keeps the edit-compile loop at minutes instead of hours, which matters
# because panel and input bring-up is many cycles of adjusting this file.
#
# It also makes the blob a first-class artifact that a check can inspect, so
# the values measured off the hardware can be asserted rather than trusted.
{
  stdenv,
  lib,
  kernel,
  bc,
  bison,
  flex,
  perl,
  python3,
}:

let
  dtbName = "rk3326-aislpc-r36t-max";
in
stdenv.mkDerivation {
  pname = "${dtbName}-dtb";
  inherit (kernel) version src;

  nativeBuildInputs = [
    bc
    bison
    flex
    perl
    python3
  ];

  postPatch = ''
    cp ${./rk3326-aislpc-r36t-max.dts} arch/arm64/boot/dts/rockchip/${dtbName}.dts
    echo 'dtb-$(CONFIG_ARCH_ROCKCHIP) += ${dtbName}.dtb' \
      >> arch/arm64/boot/dts/rockchip/Makefile
  '';

  # The arm64 defconfig sets ARCH_ROCKCHIP, which is all the device-tree
  # target needs. Building every arm64 DTB is wasteful but reliable: naming a
  # single blob as a make target has moved between kernel versions.
  buildPhase = ''
    runHook preBuild

    make ARCH=arm64 defconfig
    make ARCH=arm64 -j"$NIX_BUILD_CORES" dtbs

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    install -Dm444 arch/arm64/boot/dts/rockchip/${dtbName}.dtb \
      "$out/rockchip/${dtbName}.dtb"

    runHook postInstall
  '';

  passthru = { inherit dtbName; };

  meta = {
    description = "Device tree blob for the AISLPC R36T Max";
    platforms = [ "aarch64-linux" ];
    license = lib.licenses.gpl2Plus;
  };
}
