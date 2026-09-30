{ pkgs }:
let
  upstream = pkgs.sway-unwrapped;
  patched = upstream.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ./sway-idle-notify.patch ];
  });
in
# Keep the flake's upstream source pin and all upstream dependencies/patches.
# Review the native idle boundary again when upgrading Sway.
assert upstream.version == "1.12";
assert upstream.src.outputHash == "sha256-OcF7jOOHhFPhM5APn5riy8S5jsEr9jALCVh9nBtD7Nk=";
pkgs.sway.override { sway-unwrapped = patched; }
