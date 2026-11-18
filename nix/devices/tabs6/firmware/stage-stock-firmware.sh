# Add the three stock firmware members that ./default.nix needs to the
# local Nix store, as fixed-output paths. Reads only; the tars are not changed.
#
#   tabs6-stage-firmware <dir holding the BL_ and AP_ T860XXS5DWH1 tars>
dir=${1:?usage: tabs6-stage-firmware <stock firmware dir>}
bl=$(find "$dir" -maxdepth 1 -name 'BL_T860XXS5DWH1_*.tar.md5' | head -n1)
ap=$(find "$dir" -maxdepth 1 -name 'AP_T860XXS5DWH1_*.tar.md5' | head -n1)
[ -n "$bl" ] || { echo "no BL_T860XXS5DWH1 tar in $dir" >&2; exit 1; }
[ -n "$ap" ] || { echo "no AP_T860XXS5DWH1 tar in $dir" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tar -C "$work" -xf "$bl" NON-HLOS.bin.lz4
tar -C "$work" -xf "$ap" modem.bin.lz4 vendor.img.ext4.lz4
for f in NON-HLOS.bin.lz4 modem.bin.lz4 vendor.img.ext4.lz4; do
	nix-store --add-fixed sha256 "$work/$f"
done
