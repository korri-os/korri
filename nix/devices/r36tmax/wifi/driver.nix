{
  lib,
  stdenv,
  callPackage,
  kernel,
  kernelModuleMakeFlags,
  bc,
  buildPackages,
}:

stdenv.mkDerivation {
  pname = "rk915";
  version = "${kernel.version}-unstable-2025-07-08";
  src = callPackage ./source.nix { };

  hardeningDisable = [ "pic" ];
  nativeBuildInputs = [
    bc
  ]
  ++ kernel.moduleBuildDependencies
  ++ [
    buildPackages.kmod
    buildPackages.python3
    buildPackages.dtc
  ];
  dontPatchELF = true;
  enableParallelBuilding = true;

  # Use the configured kernel and its symbol CRCs, never the build host's kernel.
  makeFlags = kernelModuleMakeFlags ++ [
    "-C"
    "${kernel.dev}/lib/modules/${kernel.modDirVersion}/build"
    "CONFIG_RK915=m"
  ];
  preBuild = ''
    makeFlagsArray+=("M=$PWD")
  '';
  buildFlags = [ "modules" ];

  installPhase = ''
    runHook preInstall
    install -Dm644 rk915.ko "$out/lib/modules/${kernel.modDirVersion}/extra/rk915.ko"
    runHook postInstall
  '';

  # stdenv skips installCheck when cross compiling. These checks use host tools
  # to read the installed ELF; they never execute a target binary.
  postFixup = ''
    READELF=${stdenv.cc.targetPrefix}readelf \
      bash ${./check-module.sh} "$out" ${kernel.dev} ${kernel.modDirVersion}
    python3 ${./check-dtb.py} ${kernel}/dtbs/rockchip/rk3326-aislpc-r36t-max.dtb
  '';

  meta = {
    description = "Experimental Rockchip RK915 SDIO Wi-Fi driver for R36T Max";
    homepage = "https://github.com/stolen/rk915";
    license = lib.licenses.gpl2Only;
    platforms = [ "aarch64-linux" ];
  };
}
