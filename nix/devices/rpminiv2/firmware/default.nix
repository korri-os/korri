# Firmware selection follows the pinned ROCKNIX SM8250 DTS, firmware list,
# and kernel pre_make_target. See README.md for symlinks and DSP companions.
{
  lib,
  fetchurl,
  runCommand,
  xz,
  wireless-regdb,
}:
let
  # packages/linux-firmware/kernel-firmware/package.mk at distribution
  # e81d1fc943458fb13cffe1646761e9452b29ddc1 (not the older nixpkgs release).
  version = "20260622";
  src = fetchurl {
    url = "https://cdn.kernel.org/pub/linux/kernel/firmware/linux-firmware-${version}.tar.xz";
    hash = "sha256:2b9d8a358e76eb766588609135e53fa548b902c551daae33ee32f26f25e60dbb";
  };
  # Physical archive files, before WHENCE's generated links. The JSON files
  # accompany the DSP images; do not drop them when trimming the collection.
  files = [
    "qcom/a650_gmu.bin"
    "qcom/a650_sqe.fw"
    "qcom/sm8250/a650_zap.mbn"
    "qcom/sm8250/adsp.mbn"
    "qcom/sm8250/adspr.jsn"
    "qcom/sm8250/adspua.jsn"
    "qcom/sm8250/cdsp.mbn"
    "qcom/sm8250/cdspr.jsn"
    "qcom/sm8250/Thundercomm/RB5/slpi.mbn"
    "qcom/sm8250/Thundercomm/RB5/slpir.jsn"
    "qcom/sm8250/Thundercomm/RB5/slpius.jsn"
    "qcom/vpu/vpu20_p4.mbn"
    "ath11k/QCA6390/hw2.0/amss.bin"
    "ath11k/QCA6390/hw2.0/board-2.bin"
    "ath11k/QCA6390/hw2.0/m3.bin"
    "qca/htbtfw20.tlv"
    "qca/htnv20.bin"
    "rtl_nic/rtl8153a-4.fw"
  ];
  links = [
    # WHENCE links used by the Venus driver.
    {
      path = "qcom/vpu-1.0/venus.mbn";
      target = "../vpu/vpu20_p4.mbn";
    }
    {
      path = "qcom/vpu-1.0/venus.mdt";
      target = "../vpu/vpu20_p4.mbn";
    }
  ]
  ++
    map
      (name: {
        # ROCKNIX copies RB5 SLPI files into sm8250/ for the common DTS's name.
        path = "qcom/sm8250/${name}";
        target = "Thundercomm/RB5/${name}";
      })
      [
        "slpi.mbn"
        "slpir.jsn"
        "slpius.jsn"
      ];
  notices = [ "ath11k/QCA6390/hw2.0/Notice.txt" ];
  regulatoryFiles = [
    "regulatory.db"
    "regulatory.db.p7s"
  ];
in
runCommand "rpminiv2-firmware-${version}"
  {
    nativeBuildInputs = [ xz ];
    passthru.firmwarePaths = files ++ map (link: link.path) links ++ regulatoryFiles;
    meta = {
      description = "Retroid Pocket Mini V2 GPU, DSP, WiFi, Bluetooth and Venus firmware";
      license = lib.licenses.unfreeRedistributableFirmware;
      sourceProvenance = [ lib.sourceTypes.binaryFirmware ];
      platforms = [ "aarch64-linux" ];
    };
  }
  ''
    mkdir -p "$out/lib/firmware" "$out/share/doc/rpminiv2-firmware"
    # Extract only the board subset. No dependency on a full installed
    # linux-firmware output, and no compressed blobs (the config disables it).
    tar -xJf ${src} --strip-components=1 ${
      lib.escapeShellArgs (
        map (path: "linux-firmware-${version}/${path}") (
          files
          ++ notices
          ++ [
            "WHENCE"
            "LICENSE"
            "LICENSES"
          ]
        )
      )
    }
    ${lib.concatMapStringsSep "\n" (path: ''
      install -Dm444 ${lib.escapeShellArg path} "$out/lib/firmware/${path}"
    '') (files ++ notices)}
    ${lib.concatMapStringsSep "\n" (link: ''
      mkdir -p "$out/lib/firmware/$(dirname ${lib.escapeShellArg link.path})"
      ln -s ${lib.escapeShellArg link.target} "$out/lib/firmware/${link.path}"
      test -s "$out/lib/firmware/${link.path}"
    '') links}
    ${lib.concatMapStringsSep "\n" (path: ''
      install -Dm444 ${wireless-regdb}/lib/firmware/${path} "$out/lib/firmware/${path}"
    '') regulatoryFiles}
    cp -r WHENCE LICENSE LICENSES "$out/share/doc/rpminiv2-firmware/"
  ''
