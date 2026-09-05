{
  lib,
  stdenv,
  fetchFromGitHub,
  cmake,
  pkg-config,
}:

stdenv.mkDerivation {
  pname = "rockchip-mpp";
  version = "2026-08-25-0986d012";

  src = fetchFromGitHub {
    owner = "rockchip-linux";
    repo = "mpp";
    rev = "0986d01294d5c2449c14cf13af9b740368c33967";
    hash = "sha256-kr+4pcV9Ch2oUg3DFUJ5WrvV0xCV2HfO9o/KABg/n3Q=";
  };

  patches = [ ./patches/rockchip-mpp-card0-first.patch ];

  nativeBuildInputs = [
    cmake
    pkg-config
  ];

  postPatch = ''
    for pc in pkgconfig/rockchip_mpp.pc.cmake pkgconfig/rockchip_vpu.pc.cmake; do
      substituteInPlace "$pc" \
        --replace-fail 'libdir=''${prefix}/@CMAKE_INSTALL_LIBDIR@' \
                       'libdir=@CMAKE_INSTALL_FULL_LIBDIR@' \
        --replace-fail 'includedir=''${prefix}/@CMAKE_INSTALL_INCLUDEDIR@' \
                       'includedir=@CMAKE_INSTALL_FULL_INCLUDEDIR@'
    done
  '';

  cmakeFlags = [
    (lib.cmakeBool "BUILD_SHARED_LIBS" true)
    (lib.cmakeBool "BUILD_TEST" true)
    "-DCMAKE_BUILD_TYPE=Release"
  ];

  meta = {
    description = "Rockchip Media Process Platform library and hardware tests";
    homepage = "https://github.com/rockchip-linux/mpp";
    license = lib.licenses.asl20;
    platforms = [ "aarch64-linux" ];
  };
}
