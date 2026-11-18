# Device tree for the AYN Thor (Qualcomm SM8550 / QCS8550).
#
# The Thor runs the Odin 2 Portal kernel unchanged: the ROCKNIX SM8550 kernel
# and its .config serve every AYN SM8550 board, and the Thor's drivers are
# already in that config (see ../README.md, "Kernel"). Only the DTB differs.
# This derivation builds that one DTB from the same patched source the Odin
# kernel compiles: the same tarball, the same patch list in the same order,
# the same .config, and the shared qcs8550-ayn-common.dtsi from the Odin tree.
# It compiles no kernel, so it needs neither the Odin kernel build nor the
# memory a kernel compile takes.
#
# Provenance: ROCKNIX distribution rev 1178bc2238de782bf081c558c177d35bb3690021
# (branch next, 2026-09-04), the Odin kernel's rev.
#
#   dts/qcs8550-ayn-thor.dts   projects/ROCKNIX/devices/SM8550/linux/dts/qcom/qcs8550-ayn-thor.dts
#                              (unchanged; sha256 ff97024cd465...591add)
#
# DTC_FLAGS=-@ matches the flag the NixOS kernel builder passes to `dtbs`, so
# the blob carries symbols for overlays like the Odin DTB.
{
  lib,
  stdenv,
  buildPackages,
  bison,
  flex,
  perl,
  bc,
  python3Minimal,
  odinKernel,
}:

let
  dtbName = "qcs8550-ayn-thor";
  makeFlags = [
    "ARCH=${stdenv.hostPlatform.linuxArch}"
    "CROSS_COMPILE=${stdenv.cc.targetPrefix}"
    "HOSTCC=${buildPackages.stdenv.cc.targetPrefix}cc"
  ];
in
stdenv.mkDerivation {
  pname = "thor-dtb";
  inherit (odinKernel) version src patches;
  __structuredAttrs = true;

  depsBuildBuild = [ buildPackages.stdenv.cc ];
  nativeBuildInputs = [
    bison
    flex
    perl
    bc
    python3Minimal
  ];

  inherit makeFlags;

  postPatch = ''
    patchShebangs scripts
    cp ${../../odin2portal/kernel/dts}/qcs8550-ayn-common.dtsi arch/arm64/boot/dts/qcom/
    cp ${./dts}/${dtbName}.dts arch/arm64/boot/dts/qcom/
    echo 'dtb-$(CONFIG_ARCH_QCOM) += ${dtbName}.dtb' >> arch/arm64/boot/dts/qcom/Makefile
  '';

  configurePhase = ''
    runHook preConfigure
    cp ${odinKernel.configfile} .config
    chmod u+w .config
    make "''${makeFlags[@]}" olddefconfig
    runHook postConfigure
  '';

  buildPhase = ''
    runHook preBuild
    make "''${makeFlags[@]}" DTC_FLAGS=-@ qcom/${dtbName}.dtb
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    install -Dm444 arch/arm64/boot/dts/qcom/${dtbName}.dtb "$out/dtbs/qcom/${dtbName}.dtb"
    runHook postInstall
  '';

  passthru = { inherit dtbName; };

  meta = {
    description = "AYN Thor device tree built from the Odin 2 Portal's ROCKNIX SM8550 kernel source";
    license = lib.licenses.gpl2Only;
    platforms = [ "aarch64-linux" ];
  };
}
