{
  pkgs,
  inputdPackage,
  deviceRoot ? "/dev/input",
  sysRoot ? "/sys",
  setfacl ? "${pkgs.acl}/bin/setfacl",
}:
pkgs.writeShellApplication {
  name = "korri-virtual-target-acl";
  text = ''
    exec ${inputdPackage}/bin/korri-virtual-target-acl \
      --device-root ${pkgs.lib.escapeShellArg deviceRoot} \
      --sys-root ${pkgs.lib.escapeShellArg sysRoot} \
      --setfacl ${pkgs.lib.escapeShellArg setfacl} \
      "$@"
  '';
}
