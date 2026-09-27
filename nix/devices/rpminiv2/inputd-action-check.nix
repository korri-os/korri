# Run inputd's real startup validator on the emitted ARM board action. The
# native test runner inspects metadata only; it never executes ARM swaymsg.
{
  pkgs,
  inputd,
  configuration,
}:
let
  action = pkgs.writeText "rpminiv2-controller-activity.json" configuration.config.systemd.services.korri-inputd.environment.KORRI_INPUTD_CONTROLLER_ACTIVITY;
  runner = inputd.overrideAttrs (old: {
    pname = "rpminiv2-inputd-action-runner";
    preConfigure = (old.preConfigure or "") + ''
      # Keep test-only sources out of inputd's production source derivation.
      cp ${./inputd-action-check.rs} tests/rpminiv2_inputd_action.rs
    '';
    buildPhase = ''
      runHook preBuild
      cargo test --release --offline --locked --test rpminiv2_inputd_action --no-run
    '';
    doCheck = false;
    postInstall = "";
    installPhase = ''
      mkdir -p $out/bin
      find target -type f -name 'rpminiv2_inputd_action-*' -executable \
        -exec cp {} $out/bin/rpminiv2-inputd-action \;
      test -x $out/bin/rpminiv2-inputd-action
    '';
  });
in
pkgs.runCommand "rpminiv2-inputd-action-check"
  {
    KORRI_TEST_CONTROLLER_ACTIVITY_FILE = action;
    KORRI_TEST_WRAPPED_SWAYMSG = "${configuration.pkgs.sway}/bin/swaymsg";
  }
  ''
    mkdir -p $out
    ${runner}/bin/rpminiv2-inputd-action --nocapture 2>&1 | tee $out/results.txt
    cp ${action} $out/controller-activity.json
  ''
