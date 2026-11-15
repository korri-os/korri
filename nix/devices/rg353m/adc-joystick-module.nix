# Build only the ADC joystick driver against the selected kernel, like the
# board's panel fix. Mainline looks up inversion bounds by channel index even
# though the RG353M channel order is X/RX/Y/RY, not the ABS code order.
{
  lib,
  stdenv,
  kernel,
  python3,
  ...
}:
stdenv.mkDerivation {
  pname = "adc-joystick-rg353m";
  inherit (kernel) version;
  src = kernel.src;
  patches = [ ./patches/adc-joystick-axis-bounds.patch ];
  nativeBuildInputs = kernel.moduleBuildDependencies ++ [ python3 ];

  buildPhase = ''
    runHook preBuild
    workdir="$NIX_BUILD_TOP/module"
    mkdir -p "$workdir"
    cp drivers/input/joystick/adc-joystick.c "$workdir/"
    echo 'obj-m := adc-joystick.o' > "$workdir/Makefile"
    make -C ${kernel.dev}/lib/modules/${kernel.modDirVersion}/build \
      M="$workdir" modules
    runHook postBuild
  '';

  # Exercise both real conversion functions from this exact patched source.
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    python3 ${./test-adc-joystick.py} drivers/input/joystick/adc-joystick.c "$CC"
    runHook postCheck
  '';

  installPhase = ''
    runHook preInstall
    install -Dm444 "$NIX_BUILD_TOP/module/adc-joystick.ko" \
      "$out/lib/modules/${kernel.modDirVersion}/updates/adc-joystick.ko"
    runHook postInstall
  '';

  meta = {
    description = "RG353M ADC joystick with ABS-code inversion and bounded calibration";
    license = lib.licenses.gpl2Only;
    platforms = [ "aarch64-linux" ];
  };
}
