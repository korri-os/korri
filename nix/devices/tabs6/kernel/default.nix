# Mainline Linux 7.3 for the Galaxy Tab S6 Wi-Fi (SM-T860, gts6lwifi, SM8150),
# cross-compiled from x86_64. Ported from upstream/gts6l-mainline-linux.
#
# Outputs:
#   kernel        linuxManualConfig derivation whose out holds Image (EFI stub),
#                 dtbs/qcom/sm8150-samsung-gts6lwifi.dtb, lib/modules/<version>,
#                 config and System.map. From NixOS:
#                   boot.kernelPackages = pkgs.linuxPackagesFor kernel;
#   kernelConfig  the resolved .config (base + config/tabs6.config, olddefconfig).
#   kernelSource  the patched source tree.
#
# `productFragment` (Korri) merges one more fragment after tabs6.config and
# checks it the same way. Without it every derivation is the one the tabs6
# repository builds, byte for byte, so the console kernel stays the
# hardware-proven one.
{
  pkgs,
  cross,
  productFragment ? null,
}:
let
  inherit (cross) lib;
  product = productFragment != null;
  namePart = lib.optionalString product "-korri";

  # nixpkgs' latest mainline (7.3.y at the flake pin). The port's patches were
  # written against 6.12.0; README.md records how each one moved to 7.3.
  base = cross.linux_latest;
  inherit (base) version;

  dtbName = "sm8150-samsung-gts6lwifi";

  # Order matters only where two patches touch one file; none do today.
  patches = map (name: ./patches + "/${name}.patch") [
    # Harvested from upstream/gts6l-mainline-linux/kernel/patches.
    "msm-poweroff-release-ps-hold-early"
    "qcom-pon-ps-hold-shutdown-and-smpl"
    "mfd-qcom-spmi-pmic-tolerate-denied-revid"
    "adc-battery-report-capacity-and-status"
    "cs48l32-accept-cs48l33"
    "sm8250-sndcard-sm8150-secondary-tdm"
    # Ours.
    "clk-qcom-gcc-sm8150-branch-halt-skip"
    "regulator-qcom-rpmh-no-reads"
    "drm-msm-a640-speedbin-fallback"
    "drm-simpledrm-64bit-dma-mask"
    "clk-qcom-dispcc-sm8250-byte-clk-parent-enable"
    "drm-panel-wire-samsung-ana38401"
    "input-touchscreen-wire-fts1ba90a"
    "power-supply-wire-sm5705-fuelgauge"
    "arm64-dts-qcom-wire-gts6lwifi"
  ];

  kernelSource = pkgs.applyPatches {
    name = "linux-${version}-gts6lwifi-source";
    inherit (base) src;
    inherit patches;
    postPatch = ''
      install -m644 ${./dts + "/${dtbName}.dts"} arch/arm64/boot/dts/qcom/${dtbName}.dts
      install -m644 ${./drivers/fts1ba90a.c} drivers/input/touchscreen/fts1ba90a.c
      install -m644 ${./drivers/sm5705_fuelgauge.c} drivers/power/supply/sm5705_fuelgauge.c
      install -m644 ${./drivers/panel-samsung-ana38401.c} drivers/gpu/drm/panel/panel-samsung-ana38401.c
    '';
  };

  # Base config + fragment, resolved by the kernel's own Kconfig. The build
  # fails if any fragment line does not survive olddefconfig verbatim.
  kernelConfig = cross.stdenv.mkDerivation (
    {
      pname = "linux-gts6lwifi${namePart}-config";
      inherit version;
      src = kernelSource;
      depsBuildBuild = [ cross.buildPackages.stdenv.cc ];
      nativeBuildInputs = with cross.buildPackages; [
        bison
        flex
        perl
        bc
      ];
      __structuredAttrs = true;
      makeFlags = base.commonMakeFlags;
      dontConfigure = true;
      buildPhase = ''
        runHook preBuild
        patchShebangs scripts
        cp ${./config/gts6lwifi_defconfig-full} .config
        chmod u+w .config
        KCONFIG_CONFIG=.config scripts/kconfig/merge_config.sh -m -O . .config ${./config/tabs6.config}
        make "''${makeFlags[@]}" olddefconfig

        fail=0
        while IFS= read -r line; do
          case "$line" in
            CONFIG_*=*) grep -qxF -- "$line" .config || { echo "config: wanted '$line', got '$(grep -E "^(# )?''${line%%=*}[= ]" .config || echo unset)'"; fail=1; } ;;
            "# CONFIG_"*" is not set")
              sym=''${line#\# }; sym=''${sym%% *}
              if grep -qE "^$sym=" .config; then echo "config: wanted '$line', got '$(grep -E "^$sym=" .config)'"; fail=1; fi ;;
          esac
        done < ${./config/tabs6.config}
        [ "$fail" = 0 ]
        runHook postBuild
      '';
      installPhase = ''
        cp .config $out
      '';
    }
    // lib.optionalAttrs product {
      # The same merge and the same verbatim check, for the product delta. The
      # tabs6.config lines are checked again: the delta must not undo one.
      postBuild = ''
        KCONFIG_CONFIG=.config scripts/kconfig/merge_config.sh -m -O . .config ${productFragment}
        make "''${makeFlags[@]}" olddefconfig

        fail=0
        for fragment in ${./config/tabs6.config} ${productFragment}; do
          while IFS= read -r line; do
            case "$line" in
              CONFIG_*=*) grep -qxF -- "$line" .config || { echo "config: wanted '$line', got '$(grep -E "^(# )?''${line%%=*}[= ]" .config || echo unset)'"; fail=1; } ;;
              "# CONFIG_"*" is not set")
                sym=''${line#\# }; sym=''${sym%% *}
                if grep -qE "^$sym=" .config; then echo "config: wanted '$line', got '$(grep -E "^$sym=" .config)'"; fail=1; fi ;;
            esac
          done < "$fragment"
        done
        [ "$fail" = 0 ]
      '';
    }
  );

  # One output tree holds Image, dtbs/ and lib/modules/ (nixpkgs splits modules
  # into a third output; NixOS reads `lib.getOutput "modules"`, which falls back
  # to out). It stays a real linuxManualConfig derivation, so
  # `pkgs.linuxPackagesFor kernel` and NixOS's `.override` work on it.
  #
  # The build refuses to finish unless the artefacts are what the boot chain
  # needs: an arm64 EFI-stub Image (PE "MZ" + arm64 magic) and a DTB that
  # carries the board's compatible, the bootloader framebuffer and the
  # relocated wlan_mem.
  kernel =
    (cross.linuxManualConfig {
      inherit version;
      modDirVersion = version;
      src = kernelSource;
      configfile = kernelConfig;
      # Parse the resolved .config so NixOS sees MODULES etc. (import from derivation).
      allowImportFromDerivation = true;
      extraMeta.platforms = [ "aarch64-linux" ];
    }).overrideAttrs
      (old: {
        pname = "linux-gts6lwifi${namePart}";
        outputs = [
          "out"
          "dev"
        ];
        installFlags = map (
          flag:
          if lib.hasPrefix "INSTALL_MOD_PATH=" flag then "INSTALL_MOD_PATH=${placeholder "out"}" else flag
        ) old.installFlags;
        nativeBuildInputs = old.nativeBuildInputs ++ [ cross.buildPackages.dtc ];
        postInstall = old.postInstall + ''
          # modules_install points build/ at the sandbox build tree.
          rm -f "$out/lib/modules/${version}/build"
          ln -s ${kernelConfig} "$out/config"

          img="$out/Image"
          [ "$(head -c 2 "$img")" = MZ ] || { echo "Image: no PE MZ header"; exit 1; }
          [ "$(dd if="$img" bs=1 skip=56 count=4 status=none)" = "$(printf 'ARM\x64')" ] \
            || { echo "Image: no arm64 magic"; exit 1; }

          dtb="$out/dtbs/qcom/${dtbName}.dtb"
          dtc -I dtb -O dts -o /dev/null "$dtb" 2>/dev/null || { echo "dtb: does not decompile"; exit 1; }
          want() { [ "$2" = "$3" ] || { echo "dtb: $1 is '$2', want '$3'"; exit 1; }; }
          want compatible "$(fdtget "$dtb" / compatible)" "samsung,gts6lwifi qcom,sm8150p qcom,sm8150"
          want framebuffer "$(fdtget "$dtb" /framebuffer@9c400000 compatible)" "simple-framebuffer"
          want framebuffer.reg "$(fdtget -t x "$dtb" /framebuffer@9c400000 reg)" "0 9c400000 0 fa0000"
          want panel "$(fdtget "$dtb" "$(fdtget "$dtb" /__symbols__ mdss_dsi0)/panel@0" compatible)" "samsung,ana38401-amsa05rb06"
          want mdss.status "$(fdtget "$dtb" "$(fdtget "$dtb" /__symbols__ mdss)" status)" "okay"
          [ -e "$out/lib/modules/${version}/kernel/drivers/gpu/drm/panel/panel-samsung-ana38401.ko" ] \
            || { echo "panel-samsung-ana38401.ko not built"; exit 1; }
          want wlan_mem.reg "$(fdtget -t x "$dtb" "$(fdtget "$dtb" /__symbols__ wlan_mem)" reg)" "0 c0000000 0 100000"
          [ -d "$out/lib/modules/${version}/kernel" ] || { echo "no modules installed"; exit 1; }
        '';
      });
in
{
  inherit
    kernel
    kernelConfig
    kernelSource
    ;
}
