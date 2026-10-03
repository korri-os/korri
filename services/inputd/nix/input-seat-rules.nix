# SeatSpec::for_slot and UinputSeatBackend own this identity. Generate exact
# canonical P1..P255 matches, not a wildcard that also admits P0/P01/P256.
# Both evdev and joydev expose the same seats; restrict both to gameplay.
{ pkgs, eventGid }:
pkgs.writeTextFile {
  name = "korri-input-seat-rules";
  destination = "/lib/udev/rules.d/99-z-korri-input-seat.rules";
  text =
    pkgs.lib.concatMapStringsSep "\n" (
      slot:
      let
        number = toString slot;
      in
      # udev refuses GROUP= for normal-user GIDs. Preserve the receiver's exact
      # GID/mode readiness contract with the existing chgrp/chmod mechanism.
      # Never mutate a vanished or reused node from a delayed remove event.
      ''ACTION=="add|change", SUBSYSTEM=="input", KERNEL=="event*|js*", ATTRS{name}=="Korri Seat P${number}", ATTRS{phys}=="korri/input-seat/p${number}", ATTRS{id/bustype}=="0003", ATTRS{id/vendor}=="045e", ATTRS{id/product}=="028e", ATTRS{id/version}=="0001", TAG-="uaccess", OWNER="root", MODE="0600", RUN+="${pkgs.coreutils}/bin/chgrp ${toString eventGid} $env{DEVNAME}", RUN+="${pkgs.coreutils}/bin/chmod 0660 $env{DEVNAME}"''
    ) (pkgs.lib.range 1 255)
    + "\n";
}
