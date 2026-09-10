{
  stdenv,
  lib,
  fetchFromGitHub,
  pkg-config,
  nasm,
  perl,
  libdrm,
  zlib,
  bzip2,
  xz,
  rockchipMpp,
}:

stdenv.mkDerivation (finalAttrs: {
  pname = "ffmpeg-rockchip";
  version = "8.0-unstable-2026-08-10";

  src = fetchFromGitHub {
    owner = "nyanmisaka";
    repo = "ffmpeg-rockchip";
    rev = "d90e3a1c18d7929383cf88c1b3da2e2d1c966cbf";
    hash = "sha256-epRkvUU1XO+ZHvvRV1gA7Mbsm91m3OgYFSrzxmdTccc=";
  };

  nativeBuildInputs = [
    pkg-config
    nasm
    perl
  ];

  buildInputs = [
    rockchipMpp
    libdrm
    zlib
    bzip2
    xz
  ];

  configureFlags = [
    "--disable-debug"
    "--disable-doc"
    "--enable-gpl"
    "--enable-version3"
    "--enable-shared"
    "--disable-static"
    "--enable-libdrm"
    "--enable-rkmpp"
  ];

  enableParallelBuilding = true;

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    $out/bin/ffmpeg -hide_banner -encoders 2>&1 | grep -F h264_rkmpp
    runHook postInstallCheck
  '';

  meta = {
    description = "FFmpeg with Rockchip MPP hardware codecs";
    homepage = "https://github.com/nyanmisaka/ffmpeg-rockchip";
    license = lib.licenses.gpl3Plus;
    platforms = [ "aarch64-linux" ];
  };
})
