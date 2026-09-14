# Local-use packaging only. Firmware redistribution permission is unresolved.
# Hashes match both the saved stock artifacts and stolen/rk915 at
# 590fe1dd3fa9569117317b2e0dcbe02c42f8419e. See README.md before building.
{
  lib,
  stdenvNoCC,
  requireFile,
}:
let
  firmware = requireFile {
    name = "rk915_fw.bin";
    sha256 = "f2d3a67176eaf06bce02a50ffc292c89c5f02fe33ba66daf18dec3389377e9e4";
    message = ''
      Supply the existing R36T Max rk915_fw.bin locally with nix-store --add-fixed sha256.
      Firmware redistribution is NOT cleared; see wifi/README.md.
    '';
  };
  patch = requireFile {
    name = "rk915_patch.bin";
    sha256 = "d63f8d62e0ae0d7b21fd7b031a6e8360bcf90b722516c7e1278ed72c2e62c275";
    message = ''
      Supply the existing R36T Max rk915_patch.bin locally with nix-store --add-fixed sha256.
      Firmware redistribution is NOT cleared; see wifi/README.md.
    '';
  };
in
stdenvNoCC.mkDerivation {
  pname = "rk915-firmware";
  version = "unstable-2025-07-08";
  dontUnpack = true;
  dontBuild = true;
  preferLocalBuild = true;
  allowSubstitutes = false;

  installPhase = ''
    runHook preInstall
    install -Dm644 ${firmware} "$out/lib/firmware/rk915_fw.bin"
    install -Dm644 ${patch} "$out/lib/firmware/rk915_patch.bin"
    runHook postInstall
  '';
  postFixup = ''
    bash ${./check-firmware.sh} "$out"
  '';

  meta = {
    description = "Locally supplied R36T Max RK915 firmware; redistribution not cleared";
    homepage = "https://github.com/stolen/rk915";
    # Conservative Nix policy, not a claim that a proprietary license was found.
    license = lib.licenses.unfree;
    platforms = lib.platforms.linux;
  };
}
