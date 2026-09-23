{ pkgs, crane }:
let
  craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
  sourceRoot = ./.;
  sourceRootString = toString sourceRoot;
  src = pkgs.lib.cleanSourceWith {
    src = sourceRoot;
    filter =
      path: type:
      (craneLib.filterCargoSources path type)
      || pkgs.lib.hasPrefix "${sourceRootString}/tests/fixtures/" (toString path)
      || toString path == "${sourceRootString}/deploy/device-check.sh";
  };
  # evdev 0.13.2 encodes UI_SET_PHYS with sizeof(char) instead of
  # sizeof(char*). The kernel rejects that ioctl with EINVAL, so both
  # inputd's routed targets and the seat receiver fail to start.
  cargoVendorDir = craneLib.vendorCargoDeps {
    src = sourceRoot;
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
    pname = "korri-inputd";
    version = "0.0.0";
    strictDeps = true;
    meta.mainProgram = "korri-inputd";
  };
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    cargoBuildExtraArgs = "--bin korri-inputd --bin korri-bundle-launch --bin korri-bundle-select --bin korri-virtual-target-acl --bin korri-sunshine-state-digest --bin korri-ledger-proof --bin korri-input-seat-receiver --lib";
    postInstall = ''
      # Keep this byte-for-byte identical to the repository gate. The rollout
      # verifies the candidate closure helper against the local source digest.
      install -Dm0555 "$src/deploy/device-check.sh" "$out/bin/korri-device-gate"
    '';
    doCheck = false;
  }
)
