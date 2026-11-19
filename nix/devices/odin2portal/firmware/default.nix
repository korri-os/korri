# Device firmware for the AYN Odin 2 Portal (Qualcomm SM8550 / QCS8550).
#
# The board needs three classes of firmware:
#
#   1. Redistributable blobs already correct in nixpkgs linux-firmware:
#      qcom/sm8550/a740_zap.mbn (Adreno 740 zap shader, named by
#      qcs8550-ayn-common.dtsi), qcom/a740_sqe.fw, and the ath12k WCN7850
#      amss.bin and m3.bin, which are byte-identical to ROCKNIX's copies.
#      Those come from `hardware.enableRedistributableFirmware`; this
#      package does not duplicate them.
#
#   2. Blobs where ROCKNIX ships a *different* file than nixpkgs, and the
#      ROCKNIX one is the copy proven on this hardware:
#      ath12k WCN7850 board-2.bin (per-board WiFi calibration data) and
#      qcom/vpu/vpu30_p4.mbn (Venus video firmware). Taking the nixpkgs
#      copies here would silently degrade WiFi and hardware video decode,
#      so these are overlaid on top of linux-firmware.
#
#   3. AYN-proprietary blobs that exist nowhere in nixpkgs: the Portal ADSP
#      images and their JSON manifests, the shared AYN CDSP images, the
#      AudioReach topologies, the aw883xx speaker-amp calibration, and the
#      Renesas USB firmware.
#
# The blobs are proprietary and are never committed to Korri. They are
# fetched by URL and content hash from the ROCKNIX distribution repository
# at the revision nix-on-rocks pins and sobo runs, so the build is
# reproducible without checking blobs into this repository. Built images do
# include these blobs under their existing firmware license classification.
#
# Provenance: ROCKNIX distribution rev f080b462f54b5807bdd16ac7cc2ab64528b038b1
# (branch next, 2026-05-13), path
# projects/ROCKNIX/devices/SM8550/filesystem/usr/lib/kernel-overlays/base/lib/firmware.
#
# Regenerate the pin list after a ROCKNIX bump with
# nix/devices/odin2portal/firmware/update-hashes.sh.
#
# A fourth class is the Adreno 740 GPU firmware. The 7.0 msm driver names
# three files for chip 0x43050a01 (a6xx_catalog.c): a740_sqe.fw,
# gmu_gen70200.bin, and a740_zap.mdt, which the driver resolves to
# sm8550/a740_zap.mbn. All three are in linux-firmware, but the release the
# pinned nixpkgs ships (20251125) predates them, so the GPU probed with no
# microcode and 3D never came up. They are fetched from the linux-firmware
# tree at tag 20260309 -- the first release carrying them -- until the
# nixpkgs pin catches up, at which point this block deletes itself.
#
# The Thor shares this package (nix/devices/thor). `variant` selects the
# board's own blobs; every other file is common to the AYN SM8550 boards.
{
  lib,
  fetchurl,
  runCommand,
  variant ? "odin2portal",
}:

let
  rev = "f080b462f54b5807bdd16ac7cc2ab64528b038b1";
  base =
    "https://raw.githubusercontent.com/ROCKNIX/distribution/${rev}"
    + "/projects/ROCKNIX/devices/SM8550/filesystem/usr/lib/kernel-overlays/base/lib/firmware";

  # Common to the AYN SM8550 boards.
  sharedFiles = [
    {
      path = "ath12k/WCN7850/hw2.0/board-2.bin";
      hash = "sha256-jMmmS69oaB7OXiaoRFYwdJ7Mv7RRe9v2gChWeHr6fC0=";
    }
    {
      path = "ath12k/WCN7850/hw2.0/regdb.bin";
      hash = "sha256-/hMn14nkyTqfZoav6l4XzQSLklaTxrmnrCw2AmM10xE=";
    }
    {
      path = "qcom/sm8550/SM8550-APS-tplg.bin";
      hash = "sha256-Lmm+J4hTY10b0nqQKYE1GIdbYIGfybTQ/hov6RwE3+M=";
    }
    {
      path = "qcom/sm8550/ayn/cdsp.mbn";
      hash = "sha256-yBZr95jzuraN78JDeiJajl/Hl5+vH187P8ixh7f0zM8=";
    }
    {
      path = "qcom/sm8550/ayn/cdsp_dtb.mbn";
      hash = "sha256-quSWvtSYfuv2lTjKIewwnGDAD/oOmM50Ou4VMd5jh9M=";
    }
    {
      path = "qcom/vpu/vpu30_p4.mbn";
      hash = "sha256-9Z/sOdxps0Ew+/C1zXLbJm5YUHEySho+OEFGuxn1PmY=";
    }
    {
      path = "renesas_usb_fw.mem";
      hash = "sha256-F3VgwiTHPQQINrFwgjSANkMOz1nooy13Ns4rgLJjT5c=";
    }
  ];

  # One board's blobs. The ROCKNIX tree also carries odin2, odin2mini,
  # ayaneo, and retroid blobs; none of them belong on these boards. The
  # board's DTS names its ADSP and speaker-amp files (firmware-name).
  variantFiles = {
    odin2portal = [
      {
        path = "qcom/sm8550/AYN-Odin2-tplg.bin";
        hash = "sha256-DlMMH+oHZ8IbObsRSfqwHBenxzbnjYSmOAP49Gd4QkE=";
      }
      {
        path = "qcom/sm8550/ayn/odin2portal/adsp.mbn";
        hash = "sha256-IlmhJGg/YK+O1UcsFgY5FgMWSuyrN5si/OUeP+Q0JmE=";
      }
      {
        path = "qcom/sm8550/ayn/odin2portal/adsp_dtb.mbn";
        hash = "sha256-cYZcm+BYxP6JA5M98Ps86aKr0VVSTDChaRc3QFSAjqM=";
      }
      {
        path = "qcom/sm8550/ayn/odin2portal/adspr.jsn";
        hash = "sha256-Zs6FMe+3l50jTg8Oq74Y0Disl9p3GooZT2E9+PByToE=";
      }
      {
        path = "qcom/sm8550/ayn/odin2portal/adsps.jsn";
        hash = "sha256-EqwvOujO0SqksGs6s6AbxNjv4ckuvqUDwALyCabsF28=";
      }
      {
        path = "qcom/sm8550/ayn/odin2portal/adspua.jsn";
        hash = "sha256-mCqZhTSV40cmuORGl/360wMzTeCmfL9s3jBsjxcxAAs=";
      }
      {
        path = "qcom/sm8550/ayn/odin2portal/aw883xx_acf.bin";
        hash = "sha256-pz7P4iOaVut60Vc/o1l3/VdVfPp8IgulQ3+I0lbJB60=";
      }
      {
        path = "qcom/sm8550/ayn/odin2portal/battmgr.jsn";
        hash = "sha256-voxvujMJCORoP7YoJ2HLCEhaMC1qZuCXIeEC40LycjY=";
      }
    ];
    thor = [
      # In the ROCKNIX tree AYN-Thor-tplg.bin is a Git symlink to
      # AYN-Odin2-tplg.bin, and raw.githubusercontent.com serves a symlink as
      # its 18-byte target name. The kernel then rejected that text as a
      # topology ("invalid header size ... size 0x12") and no sound card
      # came up. Fetch the target and install it under the Thor's name.
      {
        path = "qcom/sm8550/AYN-Thor-tplg.bin";
        source = "qcom/sm8550/AYN-Odin2-tplg.bin";
        hash = "sha256-DlMMH+oHZ8IbObsRSfqwHBenxzbnjYSmOAP49Gd4QkE=";
      }
      {
        path = "qcom/sm8550/ayn/thor/adsp.mbn";
        hash = "sha256-IlmhJGg/YK+O1UcsFgY5FgMWSuyrN5si/OUeP+Q0JmE=";
      }
      {
        path = "qcom/sm8550/ayn/thor/adsp_dtb.mbn";
        hash = "sha256-cYZcm+BYxP6JA5M98Ps86aKr0VVSTDChaRc3QFSAjqM=";
      }
      {
        path = "qcom/sm8550/ayn/thor/adspr.jsn";
        hash = "sha256-Zs6FMe+3l50jTg8Oq74Y0Disl9p3GooZT2E9+PByToE=";
      }
      {
        path = "qcom/sm8550/ayn/thor/adsps.jsn";
        hash = "sha256-EqwvOujO0SqksGs6s6AbxNjv4ckuvqUDwALyCabsF28=";
      }
      {
        path = "qcom/sm8550/ayn/thor/adspua.jsn";
        hash = "sha256-mCqZhTSV40cmuORGl/360wMzTeCmfL9s3jBsjxcxAAs=";
      }
      {
        path = "qcom/sm8550/ayn/thor/aw883xx_acf.bin";
        hash = "sha256-7LVBkBEHjpF40llj7XiAA8EcLFsSy8+qipjOfv8wKK8=";
      }
      {
        path = "qcom/sm8550/ayn/thor/battmgr.jsn";
        hash = "sha256-voxvujMJCORoP7YoJ2HLCEhaMC1qZuCXIeEC40LycjY=";
      }
    ];
  };

  # Sorted by path, so the install order is the one the Portal's single list had.
  files = lib.sort (a: b: a.path < b.path) (
    sharedFiles ++ variantFiles.${variant} or (throw "no SM8550 firmware for ${variant}")
  );

  linuxFirmwareTag = "20260309";
  linuxFirmwareBase = "https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git/plain";
  adrenoFiles = [
    {
      path = "qcom/a740_sqe.fw";
      hash = "sha256-lv7jNkJLE5EA/GC1tFqQc2Dks5Ntfh0AQGub2AykhHM=";
    }
    {
      path = "qcom/gmu_gen70200.bin";
      hash = "sha256-GipBnDkEbTFB/F/tWqf5cd4ttAzHodiWk8Pib61k3Zg=";
    }
    {
      path = "qcom/sm8550/a740_zap.mbn";
      hash = "sha256-OGu9wlrpSpOY4z51gO7P3O9sRyaCoeMSPhN0vP/nzeM=";
    }
  ];

  fetched =
    map (
      file:
      file
      // {
        src = fetchurl {
          url = "${base}/${file.source or file.path}";
          inherit (file) hash;
        };
      }
    ) files
    ++ map (
      file:
      file
      // {
        src = fetchurl {
          url = "${linuxFirmwareBase}/${file.path}?h=${linuxFirmwareTag}";
          name = baseNameOf file.path;
          inherit (file) hash;
        };
      }
    ) adrenoFiles;

  # A blob under 64 bytes is a Git symlink served as its target name, not
  # firmware (see AYN-Thor-tplg.bin above). Refuse it at build time.
  install = lib.concatMapStringsSep "\n" (file: ''
    if [ "$(stat -c %s ${file.src})" -lt 64 ]; then
      echo "${file.path}: $(stat -c %s ${file.src}) bytes, a symlink pointer, not firmware" >&2
      exit 1
    fi
    install -Dm444 ${file.src} "$out/lib/firmware/${file.path}"
  '') fetched;
in
runCommand "${variant}-firmware"
  {
    passthru.firmwarePaths = map (file: file.path) (files ++ adrenoFiles);

    meta = {
      description = "AYN ${{ odin2portal = "Odin 2 Portal"; thor = "Thor"; }.${variant}} ADSP, CDSP, Adreno 740, WiFi board, audio topology, and Venus firmware";
      # Proprietary vendor firmware redistributed by ROCKNIX. Fetched by
      # hash, never vendored into this repository.
      license = lib.licenses.unfreeRedistributableFirmware;
      platforms = [ "aarch64-linux" ];
    };
  }
  ''
    ${install}
  ''
