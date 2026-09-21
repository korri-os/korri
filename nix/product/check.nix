{
  pkgs,
  nixpkgs,
  korri,
  productModule,
  configurations,
}:
let
  referenceForSystem =
    system:
    import ./reference.nix {
      inherit nixpkgs korri productModule system;
    };
  check = import ./check-lib.nix {
    inherit korri referenceForSystem;
    inherit (pkgs) lib;
    defaultSystem = pkgs.stdenv.hostPlatform.system;
  };
  failures = check.validateAll configurations;
in
if failures != { } then
  throw ''
    Korri product check failed. This evaluation gate does not grant hardware support:
    ${check.formatFailures failures}
  ''
else
  pkgs.runCommand "korri-product-check" { } ''
    touch "$out"
  ''
