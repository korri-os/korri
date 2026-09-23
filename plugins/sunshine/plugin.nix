{
  pkgs,
  inputdPackage,
  sunshinePackage,
}:
let
  lib = pkgs.lib;
  substitute = path: replacements:
    lib.replaceStrings (builtins.attrNames replacements) (builtins.attrValues replacements) (
      builtins.readFile path
    );
  runtimePackage = pkgs.writeShellScriptBin "korri-sunshine-run" (
    substitute ./runtime.sh {
      "@coreutils@" = toString pkgs.coreutils;
      "@gnugrep@" = toString pkgs.gnugrep;
      "@jq@" = toString pkgs.jq;
      "@pipewire@" = toString pkgs.pipewire;
      "@sunshine@" = toString sunshinePackage;
      "@sway@" = toString pkgs.sway;
    }
  );
  rulesFile = pkgs.writeText "99-z-korri-sunshine-input.rules" (
    builtins.readFile ./99-z-korri-sunshine-input.rules
  );
  setupPackage = pkgs.writeShellScriptBin "korri-sunshine-input-seat-setup" (
    substitute ./input-seat-setup.sh {
      "@coreutils@" = toString pkgs.coreutils;
      "@getent@" = toString pkgs.glibc.bin;
      "@groupadd@" = toString pkgs.shadow;
      "@groupdel@" = toString pkgs.shadow;
      "@rules@" = toString rulesFile;
      "@udevadm@" = toString pkgs.systemd;
    }
  );
  unit = name: source: replacements:
    let
      package = pkgs.writeTextFile {
        name = "korri-${name}";
        destination = "/lib/systemd/system/${name}";
        text = substitute source replacements;
      };
    in
    "${package}/lib/systemd/system/${name}";
in
{
  packages = {
    sunshine = sunshinePackage;
    inputd = inputdPackage;
  };
  files = {
    sunshine = "${sunshinePackage}/bin/sunshine";
    input-seat-receiver = "${inputdPackage}/bin/korri-input-seat-receiver";
    runtime = "${runtimePackage}/bin/korri-sunshine-run";
    setup = "${setupPackage}/bin/korri-sunshine-input-seat-setup";
    input-rules = toString rulesFile;
  };
  services = {
    korri-sunshine = unit "korri-sunshine.service" ./korri-sunshine.service {
      "@runtime@" = "${runtimePackage}/bin/korri-sunshine-run";
    };
    "korri-sunshine-certificate-control.socket" =
      unit "korri-sunshine-certificate-control.socket" ./korri-sunshine-certificate-control.socket { };
    korri-sunshine-input-seat-receiver =
      unit "korri-sunshine-input-seat-receiver.service" ./korri-sunshine-input-seat-receiver.service {
        "@inputd@" = toString inputdPackage;
        "@setup@" = "${setupPackage}/bin/korri-sunshine-input-seat-setup";
      };
  };
  ports = {
    allowedTCPPorts = [ 47984 47989 48010 ];
    allowedUDPPorts = [ 47998 47999 48000 48002 48010 ];
  };
}
