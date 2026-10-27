{
  pkgs,
  crane,
  proseql,
  buildBinaries ? true,
}:
let
  package =
    identity:
    import ./package.nix {
      inherit pkgs crane proseql;
      buildIdentity = identity;
    };
  first = package "identity-regression-first";
  second = package "identity-regression-second";
in
assert first.cargoArtifacts.drvPath == second.cargoArtifacts.drvPath;
assert first.drvPath != second.drvPath;
pkgs.runCommand "korri-build-identity${if buildBinaries then "" else "-eval"}-check" { } (
  pkgs.lib.optionalString buildBinaries ''
    test "$(${first}/bin/korri --version)" = 'korri 0.0.0 (identity-regression-first)'
    test "$(${second}/bin/korri --version)" = 'korri 0.0.0 (identity-regression-second)'
  ''
  + ''
    touch "$out"
  ''
)
