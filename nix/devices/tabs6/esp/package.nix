{
  writeShellApplication,
  symlinkJoin,
  coreutils,
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
in
symlinkJoin {
  name = "tabs6-esp";
  paths = [
    tabs6-esp
    hook
  ];
}
