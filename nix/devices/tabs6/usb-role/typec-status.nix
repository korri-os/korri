# A std-only Rust observer. Built on a build machine, never on the tablet.
{ pkgs }:
pkgs.stdenv.mkDerivation {
  pname = "tabs6-typec-status";
  version = "0.1.0";
  dontUnpack = true;
  nativeBuildInputs = [ pkgs.rustc ];
  buildPhase = ''
    rustc --edition=2021 -D warnings -O ${./typec-status.rs} -o tabs6-typec-status
  '';
  installPhase = ''
    install -Dm755 tabs6-typec-status $out/bin/tabs6-typec-status
  '';
  meta.mainProgram = "tabs6-typec-status";
}
