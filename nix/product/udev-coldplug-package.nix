# Boot coldplug that replays devices plus only the module events this
# system's udev rules name.
#
# The stock systemd-udev-trigger runs `udevadm trigger --type=all`, which adds
# one event per kernel module, driver and bus beside the devices. On the RG353M
# (2026-10-01) that was 3,231 events, 2,474 of them module and driver events.
# udev works through them in order, so the USB controller bound at 25 s and the
# seat receiver's first node waited 17.7 s, past its 15 s limit. Replaying
# devices plus the module events the rules name brought the controller to
# 12.5 s and Shift from 39.5 s to 35.0 s on the same system.
#
# The program is the same on every device. The module list is a device fact:
# udev-coldplug-modules.nix derives it from the device's rules and the unit
# passes it as KORRI_UDEV_COLDPLUG_MODULES, a file with one module per line.
{
  pkgs,
  udevadm,
}:

pkgs.writeShellApplication {
  name = "korri-udev-coldplug";
  runtimeInputs = [ pkgs.coreutils ];
  text = ''
    readonly modules_file="''${KORRI_UDEV_COLDPLUG_MODULES:?}"
    rc=0
    names=()
    while IFS= read -r name; do
      [ -n "$name" ] && names+=("--sysname-match=$name")
    done < "$modules_file"
    # No named module means no module trigger at all: a module trigger without
    # a sysname match would replay every module.
    if [ "''${#names[@]}" -gt 0 ]; then
      ${udevadm} trigger --type=subsystems --action=add --subsystem-match=module "''${names[@]}" || rc=$?
    fi
    ${udevadm} trigger --type=devices --action=add --prioritized-subsystem=block,tpmrm,net,tty,input || rc=$?
    exit "$rc"
  '';
}
