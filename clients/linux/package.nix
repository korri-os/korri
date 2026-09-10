{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "korri-portal-shell";
  version = "0.0.0";
  src = pkgs.lib.fileset.toSource {
    root = ./.;
    fileset = pkgs.lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
      ./tests
    ];
  };
  # The API download route returns 403 for syn 3.0.5 on the hosted runner.
  # Change only the download endpoint, retaining Cargo's registry identity,
  # normal TLS verification and every lockfile checksum.
  cargoDeps =
    (pkgs.rustPlatform.importCargoLock.override {
      fetchurl =
        args:
        pkgs.fetchurl (
          args
          // {
            url =
              builtins.replaceStrings
                [ "https://crates.io/api/v1/crates/" ]
                [ "https://static.crates.io/crates/" ]
                args.url;
          }
        );
    })
      { lockFile = ./Cargo.lock; };
  strictDeps = true;
  # The Nix builder rejects ACL mutation. The policy test still runs here;
  # run the two real-filesystem cases with cargo test outside its sandbox.
  checkFlags = [
    "--skip"
    "credential_accepts_systemd_acl"
    "--skip"
    "credential_rejects_other_users_groups_and_world_access"
  ];
  meta = {
    description = "Private Chromium startup for the shared Korri RPC bridge";
    mainProgram = "korri-portal-shell";
    platforms = pkgs.lib.platforms.linux;
  };
}
