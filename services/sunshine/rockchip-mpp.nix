{
  stdenv,
  lib,
  fetchFromGitHub,
  cmake,
  pkg-config,
}:
stdenv.mkDerivation {
  pname = "rockchip-mpp";
  version = "1.1.0";

  src = fetchFromGitHub {
    owner = "rockchip-linux";
    repo = "mpp";
    # Official 1.1.0 tag. Nixpkgs at our pinned revision has no MPP package.
    rev = "c08762ebfadeb4e986d2fed993bc7a54862d3ebe";
    hash = "sha256-Y7oJmdug/9HtChjbasGQLjqXFABMI+1VXRn7vjxEIag=";
  };

  nativeBuildInputs = [ cmake pkg-config ];
  cmakeFlags = [
    (lib.cmakeBool "BUILD_TEST" false)
    (lib.cmakeBool "BUILD_SHARED_LIBS" true)
    # MPP's .pc template prefixes these paths itself. Keep them relative.
    "-DCMAKE_INSTALL_LIBDIR=lib"
    "-DCMAKE_INSTALL_INCLUDEDIR=include"
  ];

  meta = {
    description = "Rockchip Media Process Platform for Sunshine's RKMPP encoder";
    homepage = "https://github.com/rockchip-linux/mpp";
    license = [ lib.licenses.asl20 lib.licenses.mit ];
    platforms = [ "aarch64-linux" ];
  };
}
