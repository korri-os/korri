# Image proof registration, shared with the native lifecycle acceptance VM.
# Package bytes and receipts arrive through the existing image producers.
{
  pkgs,
  pluginPackages,
  metadata,
  publicKey,
}:
let
  packages = pkgs.lib.escapeShellArgs (map toString pluginPackages);
  nix =
    "${pkgs.nix}/bin/nix --offline --extra-experimental-features nix-command"
    + " --option substituters '' --option extra-substituters ''"
    + " --option trusted-public-keys ${pkgs.lib.escapeShellArg publicKey}"
    + " --option extra-trusted-public-keys ''";
in
{
  description = "Register signed metadata for shipped plugin closures";
  requiredBy = [ "korri-plugin-host.service" ];
  before = [ "korri-plugin-host.service" ];
  after = [ "systemd-tmpfiles-setup.service" ];
  serviceConfig = {
    Type = "oneshot";
    RemainAfterExit = true;
    RuntimeDirectory = "korri-plugin-offline-proofs";
    RuntimeDirectoryMode = "0700";
    UMask = "0077";
  };
  # Always load the genuine local proofs. Generic Nix verification can accept
  # an ultimately trusted path without proving the namespace's bound signer.
  # The host still performs its own publisher/approval checks before activation.
  # Retain first-import content hashing; later boots can reuse local proofs.
  # Never fetch a remote proof or build on the device.
  script = ''
    set -euo pipefail
    verification=(--no-contents)
    if ! ${nix} store verify --no-contents --recursive --sigs-needed 1 ${packages} >/dev/null 2>&1; then
      verification=()
    fi
    cache="$(${pkgs.coreutils}/bin/mktemp -d /run/korri-plugin-offline-proofs/offline-cache.XXXXXXXX)"
    trap '${pkgs.coreutils}/bin/rm -rf -- "$cache"' EXIT
    ${pkgs.coreutils}/bin/cp -a ${metadata}/. "$cache/"
    ${nix} store copy-sigs --recursive --substituter "file://$cache" ${packages}
    ${nix} store verify "''${verification[@]}" --recursive --sigs-needed 1 ${packages} >/dev/null
  '';
}
