{
  pkgs,
  system,
  inputplumberNixpkgs,
  crane,
  korriBundleModule,
  korriInputModule,
  korridLinuxDeviceModule,
  korriLinuxHostModule,
  korridPackage,
  sunshinePlugin,
}:

let
  inputplumberPkgs = import inputplumberNixpkgs { inherit system; };
  inputplumberRuntime = pkgs.callPackage ./inputplumber-package.nix {
    inputplumber = inputplumberPkgs.inputplumber;
  };
  inputplumberData = import ./inputplumber-data.nix { inherit pkgs; };
  inputplumberKorri = inputplumberData.compose { inherit inputplumberRuntime; };
  # Preserve the existing consumer approval reference, extracted from the
  # reviewed device gate at f6597dd6f. Publisher provenance cannot replace it.
  # Fixtures keep the native line format and do not enter runtime policy.
  sunshineApprovalFixture = ./tests/fixtures/device-gate-sunshine-approval.txt;
  sunshinePatchFixture = ./tests/fixtures/device-gate-sunshine-patches.txt;
  inputdPackage = import ../package.nix { inherit pkgs crane; };
  devApp = import ./dev-app.nix {
    inherit pkgs inputdPackage korridPackage;
  };
  korriBundle = import ./korri-bundle.nix {
    inherit
      pkgs
      inputdPackage
      inputplumberKorri
      korridPackage
      ;
  };
  toApp = package: {
    type = "app";
    program = "${package}/bin/${package.name}";
  };
in
{
  apps = {
    korri-dev = toApp devApp;
    korri-bundle-select = {
      type = "app";
      program = "${inputdPackage}/bin/korri-bundle-select";
    };
  };
  packages = {
    sway-controller-activity = import ./sway-activity.nix { inherit pkgs; };
    inputplumber-korri = inputplumberKorri;
    korri-inputd = inputdPackage;
    korri-bundle = korriBundle;
  };
  checks = {
    sway-controller-activity = import ./sway-activity-check.nix { inherit pkgs; };
    inputplumber-korri-package = import ./inputplumber-package-check.nix {
      inherit
        pkgs
        inputplumberRuntime
        inputplumberKorri
        ;
    };
    inputplumber-device-paths-vm = import ./inputplumber-device-paths-vm-test.nix {
      module = korriInputModule;
      inherit pkgs inputdPackage inputplumberKorri;
    };
    unified-controller-input-vm = import ./unified-controller-input-vm-test.nix {
      inherit pkgs inputdPackage;
    };
    korri-input-seat-receiver = import ./korri-input-seat-receiver-check.nix {
      inherit pkgs inputdPackage;
    };
    korri-inputd-package =
      pkgs.runCommand "korri-inputd-package-check" { nativeBuildInputs = [ pkgs.jq ]; }
        ''
          test -x ${inputdPackage}/bin/korri-inputd
          test -x ${inputdPackage}/bin/korri-input-seat-receiver
          test -x ${inputdPackage}/bin/korri-bundle-launch
          test -x ${inputdPackage}/bin/korri-bundle-select
          test -x ${inputdPackage}/bin/korri-device-gate
          test "$(sha256sum ${inputdPackage}/bin/korri-device-gate | cut -d' ' -f1)" = \
            "$(sha256sum ${../deploy/device-check.sh} | cut -d' ' -f1)"
          gate=${inputdPackage}/bin/korri-device-gate
          grep '^EXPECTED_SUNSHINE_' "$gate" > actual-sunshine-approval
          cmp actual-sunshine-approval ${sunshineApprovalFixture}
          grep '^patch=' "$gate" > actual-patch-manifest
          cmp actual-patch-manifest ${sunshinePatchFixture}

          # Exercise the exact resolver against the actual selected package, not
          # a producer recipe. The installed provenance determines wrapper mode.
          manifest=${sunshinePlugin}/manifest.json
          sunshine_package="$(jq -er '.packages.sunshine' "$manifest")"
          declared="$(jq -er '.files.sunshine' "$manifest")"
          test "$declared" = "$sunshine_package/bin/sunshine"
          test -f "$declared"
          provenance="$sunshine_package/share/korri/sunshine-korri/provenance"
          if grep -Fx 'cuda_enabled=1' "$provenance" >/dev/null; then
            wrapped="$sunshine_package/bin/.sunshine-wrapped"
            test ! -L "$declared"
            test -L "$wrapped"
            grep -F 'bin/.sunshine-wrapped' "$declared" >/dev/null
            running="$(readlink -f -- "$wrapped")"
          else
            grep -Fx 'cuda_enabled=0' "$provenance" >/dev/null
            test -L "$declared"
            running="$(readlink -f -- "$declared")"
          fi
          case "$running" in
            "$sunshine_package"/bin/sunshine-*) ;;
            *) echo "Sunshine did not resolve to its versioned package target" >&2; exit 1 ;;
          esac
          sed -n '/^REMOTE_SUNSHINE_PACKAGE_ROOT=/,/^}/p' "$gate" > sunshine-executable-resolver.sh
          # shellcheck disable=SC1091
          source ./sunshine-executable-resolver.sh
          if grep -Fx 'cuda_enabled=1' "$provenance" >/dev/null; then
            remote_resolve_sunshine_executable "$running" "$declared"
            test "$REMOTE_SUNSHINE_PACKAGE_ROOT" = "$sunshine_package"
          else
            # The existing deployment gate does not approve the ARM symlink
            # layout. Assert its refusal; do not expand that policy here.
            if remote_resolve_sunshine_executable "$running" "$declared"; then
              echo "device gate accepted an unsupported Sunshine layout" >&2
              exit 1
            fi
            test -z "$REMOTE_SUNSHINE_PACKAGE_ROOT"
          fi

          test -x ${inputdPackage}/bin/korri-sunshine-state-digest
          test -x ${inputdPackage}/bin/korri-ledger-proof
          test -x ${inputdPackage}/bin/korri-virtual-target-acl
          test -x ${devApp}/bin/korri-dev
          grep -F 'KORRI_INPUTD_PROFILE=development' ${devApp}/bin/korri-dev >/dev/null
          grep -F 'KORRI_INPUTD_SOURCE="$physical_input"' ${devApp}/bin/korri-dev >/dev/null
          grep -F '${korridPackage}/bin/korri-local-signer' ${devApp}/bin/korri-dev >/dev/null
          grep -F 'CREDENTIALS_DIRECTORY=$signer_credential_root' ${devApp}/bin/korri-dev >/dev/null
          grep -F 'KORRID_LOCAL_SIGNER_SOCKET="$signer_socket"' ${devApp}/bin/korri-dev >/dev/null
          grep -F 'KORRID_LOCAL_SIGNER_PUBLIC_KEY_FILE="$signer_public_key"' ${devApp}/bin/korri-dev >/dev/null
          test "$(readlink -f ${korriBundle}/bin/inputplumber)" = ${inputplumberKorri}/bin/inputplumber
          test "$(readlink -f ${korriBundle}/bin/korri-inputd)" = ${inputdPackage}/bin/korri-inputd
          test "$(readlink -f ${korriBundle}/bin/korrid)" = ${korridPackage}/bin/korrid
          test "$(readlink -f ${korriBundle}/bin/korri-local-signer)" = ${korridPackage}/bin/korri-local-signer
          test "$(readlink -f ${korriBundle}/share/inputplumber)" = ${inputplumberKorri}/share/inputplumber
          test "$(readlink -f ${korriBundle}/share/korri-input-profile)" = \
            ${inputplumberKorri}/share/inputplumber/profiles/${inputplumberData.resolvedProfile}
          touch "$out"
        '';
    korri-input-module = import ./korri-input-module-check.nix {
      module = korriInputModule;
      bundleModule = korriBundleModule;
      inherit pkgs inputdPackage inputplumberKorri;
    };
    korri-bundle-module = import ./korri-bundle-module-check.nix {
      inherit
        pkgs
        korriBundleModule
        korriInputModule
        korridLinuxDeviceModule
        inputdPackage
        inputplumberKorri
        korridPackage
        korriBundle
        ;
    };
    korrid-linux-device-module = import ../../korrid/nixos-module-check.nix {
      module = korridLinuxDeviceModule;
      bundleModule = korriBundleModule;
      inherit
        pkgs
        korridPackage
        inputdPackage
        korriBundle
        ;
    };
    korri-input-seat-core =
      (import ./korri-linux-host-module-check.nix {
        module = korriLinuxHostModule;
        inherit
          pkgs
          sunshinePlugin
          inputdPackage
          inputplumberKorri
          korridPackage
          korriBundle
          ;
      }).seatVmTest;
    korri-linux-host-module = import ./korri-linux-host-module-check.nix {
      module = korriLinuxHostModule;
      inherit
        pkgs
        sunshinePlugin
        inputdPackage
        inputplumberKorri
        korridPackage
        korriBundle
        ;
    };
  };
}
