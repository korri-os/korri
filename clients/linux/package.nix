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
  cargoLock.lockFile = ./Cargo.lock;
  strictDeps = true;
  # The Nix builder rejects ACL mutation. The policy test still runs here;
  # run the two real-filesystem cases with cargo test outside its sandbox.
  checkFlags = [
    "--skip" "credential_accepts_systemd_acl"
    "--skip" "credential_rejects_other_users_groups_and_world_access"
  ];
  meta = {
    description = "Private Chromium startup for the shared Korri RPC bridge";
    mainProgram = "korri-portal-shell";
    platforms = pkgs.lib.platforms.linux;
  };
}
