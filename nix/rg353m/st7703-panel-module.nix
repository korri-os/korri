# Patched Sitronix ST7703 panel driver for the RG353M, built out of tree.
#
# The stock driver sends SETVCOM 0x92 0x92 and SETPOWER_EXT 0x25 0x22 to the
# rg353v-panel-v2 glass. The panel accepts the sequence, reports ready, and
# draws nothing. The corrected values come from the vendor BSP device tree for
# panel ID 0x3821 and are carried by ROCKNIX.
#
# This builds only the one driver against the kernel's dev output instead of
# patching the kernel, which would force a full rebuild. The module installs
# into `updates/`, which depmod prefers over the in-tree `kernel/` copy.
{
  lib,
  stdenv,
  kernel,
  ...
}:

stdenv.mkDerivation {
  pname = "panel-sitronix-st7703-rg353m";
  inherit (kernel) version;

  src = kernel.src;

  nativeBuildInputs = kernel.moduleBuildDependencies;

  patches = [ ./patches/st7703-rg353v2-init-sequence.patch ];

  # Only the one driver source is needed; unpacking the whole tree is fine but
  # the build runs against the prepared kernel build tree.
  buildPhase = ''
    runHook preBuild

    workdir="$NIX_BUILD_TOP/module"
    mkdir -p "$workdir"
    cp drivers/gpu/drm/panel/panel-sitronix-st7703.c "$workdir/"

    # Fail loudly if the patch did not reach the init sequence.
    grep -q 'ST7703_CMD_SETVCOM, 0x7f, 0x7f' "$workdir/panel-sitronix-st7703.c"
    grep -q 'ST7703_CMD_SETPOWER_EXT, 0x26, 0x62' "$workdir/panel-sitronix-st7703.c"

    echo 'obj-m := panel-sitronix-st7703.o' > "$workdir/Makefile"

    make -C ${kernel.dev}/lib/modules/${kernel.modDirVersion}/build \
      M="$workdir" \
      modules

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    install -Dm444 \
      "$NIX_BUILD_TOP/module/panel-sitronix-st7703.ko" \
      "$out/lib/modules/${kernel.modDirVersion}/updates/panel-sitronix-st7703.ko"

    runHook postInstall
  '';

  meta = {
    description = "ST7703 panel driver with the RG353V-v2 init sequence fix";
    license = lib.licenses.gpl2Only;
    platforms = [ "aarch64-linux" ];
  };
}
