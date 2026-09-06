{ pkgs }:

let
  approved = import ./approved-patches.nix;
  patchSetMaterial =
    builtins.concatStringsSep "\n" (map (record: "${record.name} ${record.sha256}") approved.patches)
    + "\n";
  patchedSource = pkgs.applyPatches {
    name = "sunshine-offline-retirement-producer-source";
    src = pkgs.sunshine.src;
    patches = map (record: record.path) approved.patches;
  };
in
assert pkgs.sunshine.src.outputHash == approved.approvedBaseSourceHash;
assert builtins.hashString "sha256" patchSetMaterial == approved.patchSetSha256;
assert builtins.all (
  record:
  builtins.baseNameOf record.path == record.name
  && builtins.hashFile "sha256" record.path == record.sha256
) approved.patches;
pkgs.stdenv.mkDerivation {
  pname = "sunshine-retire-all-clients";
  version = "1";
  outputs = [
    "out"
    "tests"
  ];
  dontUnpack = true;
  nativeBuildInputs = [ pkgs.openssl ];
  buildInputs = [
    pkgs.openssl
    pkgs.boost
    pkgs.nlohmann_json
  ];
  buildPhase = ''
    runHook preBuild
    flags="-std=c++20 -O2 -Wall -Wextra -Werror -pedantic -pthread -DBOOST_BIND_GLOBAL_PLACEHOLDERS"
    cp ${./offline-retirement.h} offline-retirement.h
    $CXX $flags -I${patchedSource}/src -I. \
      ${./retire-all-clients.cpp} ${./offline-retirement.cpp} \
      ${patchedSource}/src/korri_certificate_control.cpp ${patchedSource}/src/crypto.cpp \
      -lcrypto -o sunshine-retire-all-clients
    # Same CLI source, renamed entry for child-process syscall auditing only.
    $CXX $flags -I${patchedSource}/src -I. -Dmain=sunshine_retirement_cli_main \
      -c ${./retire-all-clients.cpp} -o retirement-cli-test.o
    $CXX $flags -I${patchedSource}/src -I. \
      ${./test-offline-retirement.cpp} ${./offline-retirement.cpp} retirement-cli-test.o \
      ${patchedSource}/src/korri_certificate_control.cpp ${patchedSource}/src/crypto.cpp \
      -Wl,--wrap=fsync,--wrap=rename,--wrap=write,--wrap=mkostemp,--wrap=unlink \
      -Wl,--wrap=open,--wrap=openat,--wrap=close,--wrap=read,--wrap=fflush \
      -lcrypto -o test-offline-retirement
    $CXX $flags -I${patchedSource}/src \
      ${./test-certificate-control.cpp} ${patchedSource}/src/korri_certificate_control.cpp \
      ${patchedSource}/src/crypto.cpp -lcrypto -o test-certificate-control
    runHook postBuild
  '';
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=retired-one \
      -keyout client-one.key -out client-one.crt -days 1 >/dev/null 2>&1
    openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=retired-two \
      -keyout client-two.key -out client-two.crt -days 1 >/dev/null 2>&1
    openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=preserved-server \
      -keyout server.key -out server.crt -days 1 >/dev/null 2>&1
    ./test-certificate-control client-one.crt client-two.crt server.crt
    # Nix's syscall filter denies setxattr. The tests output runs the full test
    # on a real temporary filesystem outside that filter, with no production data.
    ./test-offline-retirement client-one.crt client-two.crt server.crt server.key "$PWD/sunshine-retire-all-clients" "$NIX_BUILD_TOP" --sandbox-no-xattrs
    ./sunshine-retire-all-clients --help
    if ./sunshine-retire-all-clients; then exit 1; fi
    runHook postCheck
  '';
  installPhase = ''
    runHook preInstall
    install -Dm755 sunshine-retire-all-clients "$out/bin/sunshine-retire-all-clients"
    install -Dm755 test-offline-retirement "$tests/bin/test-offline-retirement"
    install -Dm755 test-certificate-control "$tests/bin/test-certificate-control"
    install -Dm444 ${./OFFLINE-RETIREMENT.md} "$out/share/doc/sunshine-retire-all-clients/README.md"
    printf '%s\n' '${approved.patchSetSha256}' > "$out/share/doc/sunshine-retire-all-clients/producer-patch-set-sha256"
    runHook postInstall
  '';
  meta = {
    description = "Offline all-client retirement using the pinned Sunshine state producer";
    mainProgram = "sunshine-retire-all-clients";
    platforms = pkgs.lib.platforms.linux;
  };
}
