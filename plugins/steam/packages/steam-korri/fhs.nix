{ pkgs, helpers }:
pkgs.buildFHSEnv {
  name = "korri-steam-arm64-fhs";
  executableName = "steam-arm64-fhs";
  privateTmp = true;
  includeClosures = true;

  targetPkgs =
    p: with p; [
      bash
      bubblewrap
      coreutils
      curl
      dbus
      fex
      file
      findutils
      gawk
      glibc.bin
      gnugrep
      gnused
      gnutar
      gzip
      lsb-release
      lsof
      pciutils
      python3
      util-linux
      usbutils
      xdg-utils
      xorg.xrandr
      xz
      zenity
    ];

  multiPkgs =
    p: with p; [
      alsa-lib
      at-spi2-core
      cairo
      cups.lib
      curl
      dbus.lib
      expat
      fontconfig
      freetype
      fribidi
      gdk-pixbuf
      glib
      glibc
      gtk2
      harfbuzz
      libcap
      libdrm
      libgbm
      libGL
      libpulseaudio
      libudev0-shim
      libva
      libxcrypt
      libxkbcommon
      libxml2
      networkmanager
      nspr
      nss
      openal
      openssl
      pango
      pipewire
      sdl2-compat
      sqlite
      udev
      vulkan-loader
      wayland
      xorg.libICE
      xorg.libSM
      xorg.libX11
      xorg.libXcomposite
      xorg.libXcursor
      xorg.libXdamage
      xorg.libXext
      xorg.libXfixes
      xorg.libXi
      xorg.libXinerama
      xorg.libXrandr
      xorg.libXrender
      xorg.libXScrnSaver
      xorg.libXtst
      xorg.libxcb
      xorg.libxshmfence
      zlib
    ];

  profile = ''
    unset GIO_EXTRA_MODULES
  '';

  runScript = "${helpers}/bin/steam-guest-run";

  # Steam expects /sbin/ldconfig to exist. Copy rather than symlink to avoid
  # nested-runtime symlink loops, matching nixpkgs' Steam FHS wrapper.
  extraBuildCommands = ''
    cp -f $out/usr/{bin,sbin}/ldconfig
  '';

  extraBwrapArgs = [
    "--bind-try"
    "/tmp/dumps"
    "/tmp/dumps"
  ];
}
