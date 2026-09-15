# Module identity evidence for the R36T Max MPP VEPU2 service. It reads real
# ELF/module metadata from the same derivation the device installs, so the
# check cannot pass against a stale or differently configured build.
{
  lib,
  stdenv,
  kmod,
  rkMppServiceModule,
  ...
}:
stdenv.mkDerivation {
  pname = "r36tmax-mpp-compile-check";
  inherit (rkMppServiceModule) version;
  dontUnpack = true;
  nativeBuildInputs = [ kmod ];
  installPhase =
    let
      modules = "${rkMppServiceModule}/lib/modules/${rkMppServiceModule.version}/updates";
    in
    ''
      runHook preInstall
      mkdir -p "$out"

      # Read real ELF/module metadata, not byte-string guesses about symbols.
      modinfo ${modules}/rk_vcodec.ko > "$out/modinfo.txt"
      modinfo -F vermagic ${modules}/rk_vcodec.ko | grep '^${rkMppServiceModule.version} '
      test "$(modinfo -F import_ns ${modules}/rk_vcodec.ko)" = DMA_BUF
      $NM --defined-only ${modules}/rk_vcodec.ko > "$out/symbols.txt"
      grep -E ' [Dd] rockchip_vepu2_driver$' "$out/symbols.txt"

      # The runtime overlay is a separate module so a failed device-tree apply
      # cannot take the codec driver down with it.
      modinfo ${modules}/rk_vepu_overlay.ko > "$out/overlay-modinfo.txt"
      modinfo -F vermagic ${modules}/rk_vepu_overlay.ko | grep '^${rkMppServiceModule.version} '

      runHook postInstall
    '';
  meta = {
    description = "Module metadata check for the R36T Max MPP VEPU2 service";
    license = with lib.licenses; [
      gpl2Plus
      mit
    ];
    platforms = [ "aarch64-linux" ];
  };
}
