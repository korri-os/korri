{
  lib,
  writeShellApplication,
  writeScriptBin,
  symlinkJoin,
  bash,
  coreutils,
  e2fsprogs,
  gnugrep,
  gnused,
  util-linux,
}:
let
  tabs6-esp = writeShellApplication {
    name = "tabs6-esp";
    runtimeInputs = [
      coreutils
      gnused
      util-linux
    ];
    text = builtins.readFile ./tabs6-esp.sh;
  };
  hook = writeShellApplication {
    name = "tabs6-esp-install-hook";
    text = ''exec ${tabs6-esp}/bin/tabs6-esp install "$1"'';
  };
  # PID 1 of the offline root check entry. Not writeShellApplication: errexit
  # in PID 1 would panic the kernel on the first failed command instead of
  # restoring the boot entry. The image's bash and tools come from the store
  # on the read-only root, so no static busybox is needed.
  fsck-init = writeScriptBin "tabs6-fsck-init" ''
    #!${bash}/bin/bash
    export PATH=${
      lib.makeBinPath [
        tabs6-esp
        coreutils
        e2fsprogs
        gnugrep
        gnused
        util-linux
      ]
    }
    ${builtins.readFile ./tabs6-fsck-init.sh}
  '';
in
symlinkJoin {
  name = "tabs6-esp";
  paths = [
    tabs6-esp
    hook
    fsck-init
  ];
}
