{ pkgs }:
let
  # OpenSSH normally signals readiness after even one address binds. The host
  # must not open both firewall families for a partly occupied listener set.
  openssh = pkgs.openssh.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ./require-complete-listeners.patch ];
  });
  upstream = import ./upstream.nix { inherit pkgs openssh; };
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
      [ (toString openssh) (toString pkgs.coreutils) config ]
      (builtins.readFile path);
  preparation = pkgs.writeShellScriptBin "korri-sshd-prepare" (substitute ./prepare.sh);
  startup = pkgs.writeShellScriptBin "korri-sshd-start" (substitute ./start.sh);
  prepare = "${preparation}/bin/korri-sshd-prepare";
  start = "${startup}/bin/korri-sshd-start";
  service = pkgs.writeTextFile {
    name = "korri-sshd.service";
    destination = "/lib/systemd/system/korri-ssh.service";
    text = pkgs.lib.replaceStrings [ "@prepare@" "@start@" ] [ prepare start ] (
      builtins.readFile ./korri-ssh.service
    );
  };
in
{
  packages = { inherit openssh; };
  files = {
    sshd = "${openssh}/bin/sshd";
    ssh-keygen = "${openssh}/bin/ssh-keygen";
    inherit config prepare start;
  };
  services.korri-ssh = "${service}/lib/systemd/system/korri-ssh.service";
  inherit (upstream) ports;
}
