# The std-only Rust observer and automatic policy. Built on a build machine, never on the tablet.
{ pkgs }:
pkgs.stdenv.mkDerivation {
  pname = "tabs6-typec-status";
  version = "0.1.0";
  src = ./.;
  nativeBuildInputs = [ pkgs.rustc ];
  buildPhase = ''
    rustc --edition=2021 -D warnings -O typec-status.rs -o tabs6-typec-status
    rustc --edition=2021 -D warnings -O typec-auto.rs -o tabs6-typec-auto
  '';
  installPhase = ''
    install -Dm755 tabs6-typec-status $out/bin/tabs6-typec-status
    install -Dm755 tabs6-typec-auto $out/bin/tabs6-typec-auto
  '';
  meta.mainProgram = "tabs6-typec-status";
}
