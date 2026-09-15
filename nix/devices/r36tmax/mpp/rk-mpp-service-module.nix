{
  lib,
  stdenv,
  kernel,
  dtc,
  python3,
  ...
}:

stdenv.mkDerivation {
  pname = "rk-mpp-service-r36tmax";
  inherit (kernel) version;

  src = ../../rg353m/rk-mpp-service/src;

  nativeBuildInputs = kernel.moduleBuildDependencies ++ [
    dtc
    python3
  ];

  postPatch = ''
    cp ${./src/Makefile} Makefile
    cp ${./src/mpp_vepu2.c} mpp_vepu2.c
    cp rk_mpp_overlay.c rk_vepu_overlay.c
  '';

  buildPhase = ''
    runHook preBuild

    dtc -@ -I dts -O dtb \
      -o rk-mpp-service-overlay.dtbo \
      ${./rk-mpp-runtime-overlay.dts}

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
    install -Dm444 rk_vepu_overlay.ko \
      "$out/lib/modules/${kernel.modDirVersion}/updates/rk_vepu_overlay.ko"

    runHook postInstall
  '';

  meta = {
    description = "Rockchip MPP VEPU2 service for R36T Max";
    license = with lib.licenses; [
      gpl2Plus
      mit
    ];
    platforms = [ "aarch64-linux" ];
  };
}
