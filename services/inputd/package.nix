{ pkgs, crane }:
let
  craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
  lib = pkgs.lib;
  sourceRoot = ../..;
  sourceRootString = toString sourceRoot;
  # Keep repository-relative path dependencies without importing other services.
  src = lib.cleanSourceWith {
    src = sourceRoot;
    filter =
      path: type:
      let
        relative = lib.removePrefix "${sourceRootString}/" (toString path);
        inCrate = lib.hasPrefix "services/inputd/" relative;
        inTreaty = lib.hasPrefix "contracts/input/" relative;
      in
      toString path == sourceRootString
      || builtins.elem relative [
        "services"
        "services/inputd"
        "contracts"
        "contracts/input"
      ]
      || ((inCrate || inTreaty) && craneLib.filterCargoSources path type)
      || lib.hasPrefix "services/inputd/tests/fixtures/" relative
      || relative == "services/inputd/deploy/device-check.sh";
  };
  # evdev 0.13.2 encodes UI_SET_PHYS with sizeof(char) instead of
  # sizeof(char*). The kernel rejects that ioctl with EINVAL, so both
  # shared seat receiver fails to start.
  cargoVendorDir = craneLib.vendorCargoDeps {
    inherit src;
    cargoLock = ./Cargo.lock;
    overrideVendorCargoPackage =
      package: drv:
      if package.name == "evdev" && package.version == "0.13.2" then
        pkgs.applyPatches {
          name = "evdev-0.13.2-uinput-phys-pointer";
          src = drv;
          patches = [ ./patches/evdev-0.13.2-uinput-phys-pointer.patch ];
        }
      else
        drv;
  };
  commonArgs = {
    inherit src cargoVendorDir;
    postUnpack = ''sourceRoot+=/services/inputd'';
    pname = "korri-inputd";
    version = "0.0.0";
    strictDeps = true;
    meta.mainProgram = "korri-inputd";
  };
  cargoArtifacts = craneLib.buildDepsOnly (
    (builtins.removeAttrs commonArgs [ "src" ])
    // {
      dummySrc = craneLib.mkDummySrc {
        inherit src;
        cargoLock = ./Cargo.lock;
        extraDummyScript = ''mv "$out/Cargo.lock" "$out/services/inputd/Cargo.lock"'';
      };
    }
  );
in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    cargoBuildExtraArgs = "--bin korri-inputd --bin korri-bundle-launch --bin korri-bundle-select --bin korri-virtual-target-acl --bin korri-sunshine-state-digest --bin korri-ledger-proof --bin korri-input-seat-receiver --lib";
    postInstall = ''
      # Keep this byte-for-byte identical to the repository gate. The rollout
      # verifies the candidate closure helper against the local source digest.
      install -Dm0555 "$src/services/inputd/deploy/device-check.sh" "$out/bin/korri-device-gate"
    '';
    doCheck = false;
  }
)
