#!/usr/bin/env nix
#! nix shell github:NixOS/nixpkgs/a6531044f6d0bef691ea18d4d4ce44d0daa6e816#bash github:NixOS/nixpkgs/a6531044f6d0bef691ea18d4d4ce44d0daa6e816#coreutils github:NixOS/nixpkgs/a6531044f6d0bef691ea18d4d4ce44d0daa6e816#gnugrep github:NixOS/nixpkgs/a6531044f6d0bef691ea18d4d4ce44d0daa6e816#kmod github:NixOS/nixpkgs/a6531044f6d0bef691ea18d4d4ce44d0daa6e816#binutils github:NixOS/nixpkgs/a6531044f6d0bef691ea18d4d4ce44d0daa6e816#python3 --command bash
# Inspect a real artifact without loading code or contacting a device.
set -euo pipefail
if [[ $# != 3 ]]; then
  echo "usage: $0 MODULE-OUTPUT KERNEL-DEV KERNEL-RELEASE" >&2
  exit 2
fi
module="$1/lib/modules/$3/extra/rk915.ko"
build="$2/lib/modules/$3/build"
test -s "$module"
test -s "$build/Module.symvers"
# Check the built configuration, not only the hand-maintained input file.
for feature in NF_TABLES NFT_CT NFT_LOG NFT_COMPAT NFT_LIMIT NFT_REJECT NETFILTER_XT_MATCH_PKTTYPE; do
  grep -Eq "^CONFIG_${feature}=[ym]$" "$build/.config"
done
test "$(modinfo -F name "$module")" = rk915
test "$(modinfo -F license "$module")" = GPL
"${READELF:-readelf}" -h "$module" | grep -q 'Machine:.*AArch64'
vermagic=$(modinfo -F vermagic "$module")
test "${vermagic%% *}" = "$3"
[[ " $vermagic " == *" modversions "* ]]
for alias in 'sdio:c*v0296d5347*' 'sdio:c*v0296d5348*'; do
  modinfo -F alias "$module" | grep -Fx "$alias"
done
for parameter in down_fw_in_probe default_phy_threshold lpw_no_sleep; do
  modinfo -F parmtype "$module" | grep -Fx "$parameter:int"
done
# The vendor does not use MODULE_FIRMWARE; these are request_firmware names.
for firmware in rk915_fw.bin rk915_patch.bin; do
  grep -aqF "$firmware" "$module"
done
# Compare every imported symbol CRC with this kernel, not another 6.12 build.
versions=$(mktemp)
trap 'rm -f "$versions"' EXIT
modprobe --show-modversions "$module" >"$versions"
python3 - "$build/Module.symvers" "$versions" <<'PY'
import pathlib
import sys

exports = {
    fields[1]: int(fields[0], 16)
    for line in pathlib.Path(sys.argv[1]).read_text().splitlines()
    if (fields := line.split())
}
imports = [line.split() for line in pathlib.Path(sys.argv[2]).read_text().splitlines()]
assert imports, "module has no symbol versions"
for crc, name in imports:
    assert name in exports, f"kernel does not export {name}"
    assert int(crc, 16) == exports[name], f"kernel CRC mismatch: {name}"
print(f"verified {len(imports)} imported symbol CRCs")
PY
printf 'RK915 artifact checks passed for %s; hardware remains untested.\n' "$3"
