{ pkgs }:
# Keep nixpkgs' official release build, wrapper, sandbox and dependency set.
# Only the browser derivation changes. The flag is opt-in, not a wrapper default.
assert pkgs.lib.assertMsg (
  pkgs.chromium.version == "143.0.7499.169"
) "Rebase and re-verify the native alpha patch before updating Chromium";
pkgs.chromium.override {
  newScope =
    extra:
    pkgs.newScope (
      extra
      // {
        mkChromiumDerivation =
          buildFun:
          (pkgs.chromium.mkDerivation buildFun).overrideAttrs (old: {
            patches = (old.patches or [ ]) ++ [ ./transparent-app-window.patch ];
          });
      }
    );
}
