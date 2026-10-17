{
  fetchurl,
  lib,
  stdenvNoCC,
  protonCachyosArm64Src ? fetchurl {
    url = "https://github.com/CachyOS/proton-cachyos/releases/download/cachyos-11.0-20260601-slr/proton-cachyos-11.0-20260601-slr-arm64.tar.xz";
    hash = "sha256-Z5Oml8gVNiapwB/NISqulyvRWWodE9SCYg9kq/X8adk=";
  },
}:

stdenvNoCC.mkDerivation rec {
  pname = "proton-cachyos-arm64";
  version = "11.0-20260601-slr-arm64";

  src = protonCachyosArm64Src;

  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;

  installPhase = ''
    runHook preInstall

    install -d "$out/share/korri/proton-cachyos-arm64/dist"
    cp -a . "$out/share/korri/proton-cachyos-arm64/dist/"

    toolmanifest="$out/share/korri/proton-cachyos-arm64/dist/toolmanifest.vdf"
    if [ -f "$toolmanifest" ]; then
      tmp="$toolmanifest.tmp"
      grep -v '"require_tool_appid"' "$toolmanifest" > "$tmp"
      mv "$tmp" "$toolmanifest"
    fi

    runHook postInstall
  '';

  passthru = {
    toolName = "proton-cachyos-${version}";
    dist = "share/korri/proton-cachyos-arm64/dist";
  };

  meta = {
    description = "Vendored ARM64-native proton-cachyos compatibility tool for Korri Steam";
    license = lib.licenses.unfreeRedistributable;
    platforms = [
      "aarch64-linux"
      "x86_64-linux"
    ];
  };
}
