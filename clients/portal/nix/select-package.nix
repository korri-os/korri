{
  pkgs,
  url ? "http://127.0.0.1:8099/",
}:
let
  policy = import ./runtime-policy.nix;
in
pkgs.writeShellApplication {
  name = "korri-portal-select";
  runtimeInputs = with pkgs; [
    nix
    coreutils
    findutils
    diffutils
    curl
    util-linux
  ];
  text = ''
    exec ${pkgs.bash}/bin/bash ${./select.sh} \
      ${pkgs.lib.escapeShellArg policy.assetRoot} \
      ${pkgs.lib.escapeShellArg url} \
      ${pkgs.systemd}/bin/systemctl "$@"
  '';
}
