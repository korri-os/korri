#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix git coreutils
# Transfer the existing x86-built inputs to the ARM image builder using a
# signed native Nix binary cache. No CI key is added to the device configuration.
set -euo pipefail
if [ "$#" -ne 2 ]; then
  echo 'usage: cross-cache.sh <export|import> <directory>' >&2
  exit 2
fi
mode="${1:?usage: cross-cache.sh <export|import> <directory>}"
directory="$(realpath -m -- "${2:?cache directory is required}")"
cd "${KORRI_ROOT:?run through the odin2portal-cross-cache Nix app}"
git diff --quiet HEAD --
revision="$(git rev-parse HEAD)"
# These are the public outputs consumed as odinKernel, odinRescueKernel and
# odinFirmware by this device's default.nix. Both transfer directions use them.
packages=(
  packages.x86_64-linux.odin2portal-kernel
  packages.x86_64-linux.odin2portal-rescue-kernel
  packages.x86_64-linux.odin2portal-firmware
)

case "$mode" in
  export)
    if [ -e "$directory" ] || [ -L "$directory" ]; then
      echo "cache destination already exists: $directory" >&2
      exit 1
    fi
    umask 077
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    mkdir -p "$work/bundle/cache"
    roots=()
    index=0
    for package in "${packages[@]}"; do
      # Build sequentially. Each kernel's temporary build tree is large.
      paths="$(nix build --print-out-paths --no-write-lock-file \
        --option pure-eval true --out-link "$work/result-$index" \
        "$KORRI_ROOT#$package^*")"
      mapfile -t outputs <<< "$paths"
      roots+=("${outputs[@]}")
      index=$((index + 1))
    done
    # The secret remains outside the uploaded bundle and is removed by the trap.
    nix-store --generate-binary-cache-key korri-odin2portal-ci \
      "$work/cache-key.secret" "$work/bundle/cache-key.pub"
    nix copy --to "file://$work/bundle/cache?secret-key=$work/cache-key.secret" "${roots[@]}"
    printf '%s\n' "$revision" > "$work/bundle/korri-revision.txt"
    mkdir -p "$(dirname "$directory")"
    mv "$work/bundle" "$directory"
    ;;
  import)
    if [ "$(cat "$directory/korri-revision.txt")" != "$revision" ]; then
      echo 'cross-build cache revision does not match this checkout' >&2
      exit 1
    fi
    roots=()
    for package in "${packages[@]}"; do
      # Re-evaluate expected paths from this checkout, not from downloaded metadata.
      # ${name} below is Nix interpolation, not a shell variable.
      # shellcheck disable=SC2016
      paths="$(nix eval --raw "$KORRI_ROOT#$package" \
        --apply 'p: builtins.concatStringsSep "\n" (map (name: p.${name}.outPath) p.outputs)')"
      mapfile -t outputs <<< "$paths"
      roots+=("${outputs[@]}")
    done
    nix copy --from "file://$directory/cache" \
      --option extra-trusted-public-keys "$(cat "$directory/cache-key.pub")" \
      --option require-sigs true "${roots[@]}"
    nix path-info "${roots[@]}" > /dev/null
    ;;
  *)
    echo 'expected export or import' >&2
    exit 2
    ;;
esac
