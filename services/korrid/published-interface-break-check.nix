# Negative control: break only Core's operation name. The very same published
# package and acceptance test must fail at the genuine named-operation seam.
{ pkgs, korridPackage }:
let
  acceptance = import ./published-game-launch-check.nix { inherit pkgs korridPackage; };
in
acceptance.overrideAttrs (old: {
  pname = "korrid-published-interface-break-check";
  postPatch = old.postPatch + ''
    substituteInPlace src/script.rs \
      --replace-fail 'pub const LAUNCH_PREPARE: &str = "launch.prepare";' \
                     'pub const LAUNCH_PREPARE: &str = "launch.prepare.interface-break";'
  '';
  checkPhase = ''
    runHook preCheck
    if cargo test --release --locked --offline --test published_game_launch \
      -- --ignored --exact published_mgba_route_and_launch_prepare > interface-rejection.log 2>&1; then
      cat interface-rejection.log
      echo 'published-package acceptance missed a genuine host-interface break' >&2
      exit 1
    fi
    cat interface-rejection.log
    grep -F 'test published_mgba_route_and_launch_prepare ... FAILED' interface-rejection.log
    grep -F 'native runner kind has no callable launch.prepare handler' interface-rejection.log
    runHook postCheck
  '';
  installPhase = ''
    mkdir -p "$out"
    cp interface-rejection.log "$out/"
  '';
  postInstall = "";
})
