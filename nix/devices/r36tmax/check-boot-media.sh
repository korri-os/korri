#!/usr/bin/env nix-shell
#!nix-shell -i bash -p bash coreutils
set -eu
. "$1"
r36tmax_boot_partition "$2" "$3"
