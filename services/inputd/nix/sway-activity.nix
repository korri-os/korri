{ pkgs }:
let
  upstream = pkgs.sway-unwrapped;
  patched = upstream.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ./sway-idle-notify.patch ];
  });
in
# Keep the flake's upstream source pin and all upstream dependencies/patches.
# Review the native idle boundary again when upgrading Sway.
assert upstream.version == "1.11";
assert upstream.src.outputHash == "sha256-xMrexVDpgkGnvAAglshsh7HjvcbU2/Q6JLUd5J487qg=";
pkgs.sway.override { sway-unwrapped = patched; }
