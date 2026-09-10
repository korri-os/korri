{ pkgs }:
let
  upstream = import ./upstream.nix { inherit pkgs; };
  configuration = pkgs.runCommand "korri-sshd-config" { } ''
    mkdir -p "$out/etc/ssh"
    cat ${./sshd_config} ${upstream.config} > "$out/etc/ssh/sshd_config"
    ${pkgs.buildPackages.openssh}/bin/sshd -G -T -f "$out/etc/ssh/sshd_config" > /dev/null
  '';
  config = "${configuration}/etc/ssh/sshd_config";
  substitute =
    path:
    pkgs.lib.replaceStrings
      [ "@openssh@" "@coreutils@" "@config@" ]
      [ (toString pkgs.openssh) (toString pkgs.coreutils) config ]
      (builtins.readFile path);
  preparation = pkgs.writeShellScriptBin "korri-sshd-prepare" (substitute ./prepare.sh);
  startup = pkgs.writeShellScriptBin "korri-sshd-start" (substitute ./start.sh);
  prepare = "${preparation}/bin/korri-sshd-prepare";
  start = "${startup}/bin/korri-sshd-start";
  service = pkgs.writeTextFile {
    name = "korri-sshd.service";
    destination = "/lib/systemd/system/sshd.service";
    text = pkgs.lib.replaceStrings [ "@prepare@" "@start@" ] [ prepare start ] (
      builtins.readFile ./sshd.service
    );
  };
in
{
  packages.openssh = pkgs.openssh;
  files = {
    sshd = "${pkgs.openssh}/bin/sshd";
    ssh-keygen = "${pkgs.openssh}/bin/ssh-keygen";
    inherit config prepare start;
  };
  services.sshd = "${service}/lib/systemd/system/sshd.service";
  inherit (upstream) ports;
}
