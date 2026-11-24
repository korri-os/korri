# The std-only read-only observer, manual interlock and role watcher. Built on a build machine, never on the tablet.
{ pkgs }:
pkgs.stdenv.mkDerivation {
  pname = "tabs6-typec-status";
  version = "0.1.0";
  src = ./.;
  nativeBuildInputs = [ pkgs.rustc ];
  buildPhase = ''
    rustc --edition=2021 -D warnings -O typec-status.rs -o tabs6-typec-status
    rustc --edition=2021 -D warnings -O typec-manual.rs -o tabs6-typec-manual
    rustc --edition=2021 -D warnings -O typec-role-watch.rs -o tabs6-typec-role-watch
  '';
  installPhase = ''
    install -Dm755 tabs6-typec-status $out/bin/tabs6-typec-status
    install -Dm755 tabs6-typec-manual $out/bin/tabs6-typec-manual
    install -Dm755 tabs6-typec-role-watch $out/bin/tabs6-typec-role-watch
  '';
  meta.mainProgram = "tabs6-typec-status";
}
