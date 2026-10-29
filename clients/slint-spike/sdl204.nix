# THROWAWAY SPIKE test fixture: upstream SDL 2.0.4, the version the LG B7
# ships as /usr/lib/libSDL2-2.0.so.0.4.0. Workstation only: the X11 video
# driver and software/OpenGL renderers, so the SDL adapter runs against real
# 2.0.4 code under Xvfb. It is never shipped; the TV uses its own library.
# LG's build carries webOS patches this one lacks.
{ pkgs }:
pkgs.stdenv.mkDerivation {
  pname = "sdl2-upstream";
  version = "2.0.4";
  src = pkgs.fetchurl {
    url = "https://www.libsdl.org/release/SDL2-2.0.4.tar.gz";
    sha256 = "0jqp46mxxbh9lhpx1ih6sp93k752j2smhpc0ad0q4cb3px0famfs";
  };
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = with pkgs; [
    libx11
    libxext
    libxrandr
    libxi
    libxcursor
    libxfixes
    libxrender
    libGL
  ];
  configureFlags = [
    "--disable-static"
    "--disable-audio"
    "--disable-joystick"
    "--disable-haptic"
    "--disable-video-wayland"
    "--disable-video-mir"
    "--disable-video-vulkan"
    "--disable-dbus"
    "--disable-ibus"
    "--disable-libudev"
  ];
  # 2016 C meets a 2026 compiler.
  env.NIX_CFLAGS_COMPILE = "-std=gnu99 -Wno-error -Wno-implicit-function-declaration -Wno-incompatible-pointer-types -Wno-int-conversion";
  enableParallelBuilding = true;
  # Only the shared library: its libSDL2main.a rule fails with current
  # binutils, and the adapter needs nothing else.
  buildFlags = [ "build/libSDL2.la" ];
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/lib"
    cp -P build/.libs/libSDL2-2.0.so* "$out/lib/"
    runHook postInstall
  '';
}
