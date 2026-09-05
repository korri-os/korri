# Rockchip's MPP service and RK3566/RK3568 RKVENC-v1 driver, ported as a
# kernel-bound external module. The checked-in source is a deliberately narrow
# slice of rockchip-linux/kernel; see rk-mpp-service/src/PROVENANCE.md.
{
  lib,
  stdenv,
  kernel,
  dtc,
  python3,
  ...
}:

stdenv.mkDerivation {
  pname = "rk-mpp-service-rkvenc";
  inherit (kernel) version;

  src = ./rk-mpp-service/src;

  nativeBuildInputs = kernel.moduleBuildDependencies ++ [
    dtc
    python3
  ];

  buildPhase = ''
    runHook preBuild

    dtc -@ -I dts -O dtb \
      -o rk-mpp-service-overlay.dtbo \
      ${./rk-mpp-service-overlay.dts}

    python3 - <<'PY'
    from pathlib import Path

    blob = Path("rk-mpp-service-overlay.dtbo").read_bytes()
    values = ",".join(f"0x{byte:02x}" for byte in blob)
    Path("rk_mpp_overlay_blob.h").write_text(
        "static const unsigned char rk_mpp_overlay_dtbo[] = {" + values + "};\n"
        "static const unsigned int rk_mpp_overlay_dtbo_len = " + str(len(blob)) + ";\n"
    )
    PY

    make -C ${kernel.dev}/lib/modules/${kernel.modDirVersion}/build \
      M="$PWD" \
      modules

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    install -Dm444 rk_vcodec.ko \
      "$out/lib/modules/${kernel.modDirVersion}/updates/rk_vcodec.ko"
    install -Dm444 rk_mpp_overlay.ko \
      "$out/lib/modules/${kernel.modDirVersion}/updates/rk_mpp_overlay.ko"

    runHook postInstall
  '';

  meta = {
    description = "Rockchip MPP RKVENC-v1 service for the RG353M";
    license = with lib.licenses; [
      gpl2Plus
      mit
    ];
    platforms = [ "aarch64-linux" ];
  };
}
