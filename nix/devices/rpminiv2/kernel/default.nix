# ROCKNIX distribution e81d1fc943458fb13cffe1646761e9452b29ddc1:
# projects/ROCKNIX/packages/linux/package.mk selects Linux 7.2 for SM8250.
# See README.md for the source paths, patch order and hardware-tested config baseline.
{
  lib,
  fetchurl,
  linuxManualConfig,
  rpminiFirmware,
  stdenv,
  buildPackages,
  kernelConfig ? ./config-tty-trim,
  # linuxPackagesFor supplies features when it re-invokes this function.
  features ? { },
  ...
}:
let
  version = "7.2";
  patchDir = ./patches;
  productFanMap = kernelConfig == ./config-korri;
  configPolicy = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./config
      ./config-korri
      ./config-korri.delta
      ./config-tty-trim
      ./derive-config-korri.py
      ./next-image-config.test.py
    ];
  };
  patchNames = lib.sort lib.lessThan (
    builtins.filter (name: lib.hasSuffix ".patch" name) (builtins.attrNames (builtins.readDir patchDir))
  );
  dtbName = "sm8250-retroidpocket-rpminiv2";
  kernel = linuxManualConfig {
    inherit version stdenv;
    modDirVersion = "7.2.0";
    src = fetchurl {
      url = "https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-${version}.tar.xz";
      hash = "sha256-+f7z0UwN9TgZAm9L50RZg1wqCw3L9bW72eoZ8IKUArM=";
    };
    kernelPatches =
      map (name: {
        inherit name;
        patch = patchDir + "/${name}";
      }) patchNames
      # Product-only Iris encoder controls Sunshine's h264_v4l2m2m needs on
      # SM8250 gen1 firmware. The recovery kernel stays unchanged.
      ++ lib.optionals productFanMap (
        map (name: {
          inherit name;
          patch = ./product-patches + "/${name}";
        }) [
          "9999-media-qcom-iris-add-request-key-frame-support.patch"
          "9999-media-qcom-iris-gen1-repeat-headers.patch"
        ]
      );
    configfile = kernelConfig;
    allowImportFromDerivation = true;
    extraMeta = {
      description = "Linux ${version} with the ROCKNIX SM8250 baseline for Retroid Pocket Mini V2";
      platforms = [ "aarch64-linux" ];
      license = lib.licenses.gpl2Only;
    };
  };
in
kernel.overrideAttrs (
  previous:
  {
    # V2 includes the Mini DTS, which includes the shared Retroid DTSI.
    # ROCKNIX copies these after patching; preserve that sequence.
    postPatch =
      (previous.postPatch or "")
      + ''
        cp ${./dts}/sm8250-retroidpocket-common.dtsi arch/arm64/boot/dts/qcom/
        cp ${./dts}/sm8250-retroidpocket-rpmini.dts arch/arm64/boot/dts/qcom/
        cp ${./dts}/${dtbName}.dts arch/arm64/boot/dts/qcom/
        echo 'dtb-$(CONFIG_ARCH_QCOM) += ${dtbName}.dtb' >> arch/arm64/boot/dts/qcom/Makefile
        ln -s ${rpminiFirmware}/lib/firmware external-firmware
      ''
      + lib.optionalString productFanMap ''
        chmod u+w arch/arm64/boot/dts/qcom/${dtbName}.dts
        printf '\n' >> arch/arm64/boot/dts/qcom/${dtbName}.dts
        cat ${./dts/sm8250-rpminiv2-korri-fan.dtsi} >> arch/arm64/boot/dts/qcom/${dtbName}.dts
      '';
    passthru = (previous.passthru or { }) // {
      inherit dtbName kernelConfig;
      compilerVersion = stdenv.cc.cc.version;
    };
  }
  // lib.optionalAttrs productFanMap {
    # Add no empty hook to recovery: its exact derivation must stay unchanged.
    nativeBuildInputs = (previous.nativeBuildInputs or [ ]) ++ [ buildPackages.python3 ];
    postConfigure = (previous.postConfigure or "") + ''
      python3 ${configPolicy}/next-image-config.test.py --source-directory ${configPolicy} "$buildRoot/.config"
    '';
  }
)
