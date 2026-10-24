# The migration consumes the existing graph seed producer and real store proofs.
{
  pkgs,
  hostPackage,
  mkPlugin,
}:
let
  fixtures = import ../../services/korrid/plugin-host/dependency-fixtures.nix {
    inherit pkgs mkPlugin;
  };
  registration = pkgs.closureInfo {
    rootPaths = [
      fixtures.runner
      fixtures.pack
      fixtures.transitive
    ];
  };
in
pkgs.runCommand "korri-plugin-cache-cutover-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.nix
    ];
  }
  ''
    cp ${./migrate-plugin-receipts.py} migrate-plugin-receipts.py
    cp ${./migrate-plugin-receipts.test.py} migrate-plugin-receipts.test.py
    python3 migrate-plugin-receipts.test.py ${hostPackage}/bin/korri-plugin \
      ${fixtures.runner} ${fixtures.pack} ${fixtures.transitive} ${registration}/registration
    touch "$out"
  ''
