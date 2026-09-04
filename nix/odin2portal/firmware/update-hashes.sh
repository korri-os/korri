#!/usr/bin/env nix-shell
#!nix-shell -i bash -p bash coreutils git nix
# Emit the fetchurl attrset for the Odin 2 Portal firmware overlay.
#
# Reads the blobs out of a local ROCKNIX checkout at the pinned rev and
# prints Nix source. The blobs themselves are never copied into Korri's
# git; only their URL and hash are recorded.
set -Eeuo pipefail

ROCKNIX_CHECKOUT="${ROCKNIX_CHECKOUT:-$HOME/code/rocknix-custom}"
REV="${REV:-f080b462f54b5807bdd16ac7cc2ab64528b038b1}"
BASE="projects/ROCKNIX/devices/SM8550/filesystem/usr/lib/kernel-overlays/base/lib/firmware"

# Files the Portal needs that nixpkgs linux-firmware does not already carry
# correctly. amss.bin and m3.bin are byte-identical upstream and are left to
# nixpkgs; board-2.bin and vpu30_p4.mbn differ and must come from ROCKNIX.
FILES=(
  ath12k/WCN7850/hw2.0/board-2.bin
  ath12k/WCN7850/hw2.0/regdb.bin
  qcom/sm8550/AYN-Odin2-tplg.bin
  qcom/sm8550/SM8550-APS-tplg.bin
  qcom/sm8550/ayn/cdsp.mbn
  qcom/sm8550/ayn/cdsp_dtb.mbn
  qcom/sm8550/ayn/odin2portal/adsp.mbn
  qcom/sm8550/ayn/odin2portal/adsp_dtb.mbn
  qcom/sm8550/ayn/odin2portal/adspr.jsn
  qcom/sm8550/ayn/odin2portal/adsps.jsn
  qcom/sm8550/ayn/odin2portal/adspua.jsn
  qcom/sm8550/ayn/odin2portal/aw883xx_acf.bin
  qcom/sm8550/ayn/odin2portal/battmgr.jsn
  qcom/vpu/vpu30_p4.mbn
  renesas_usb_fw.mem
)

for path in "${FILES[@]}"; do
  raw="$(git -C "$ROCKNIX_CHECKOUT" show "$REV:$BASE/$path" | sha256sum | cut -d' ' -f1)"
  sri="$(nix hash convert --hash-algo sha256 --to sri "$raw")"
  printf '    {\n      path = "%s";\n      hash = "%s";\n    }\n' "$path" "$sri"
done
