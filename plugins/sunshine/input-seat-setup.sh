set -eu

# NixOS writes the base input policy to 99-local.rules. Apply the approved
# plugin grants for uhid and seat nodes after it. /dev/uinput belongs to the
# host: the units reach it through SupplementaryGroups=uinput instead.
rules=/run/udev/rules.d/99-z-korri-sunshine-input.rules
case "${1:-}" in
  start)
    if @getent@/bin/getent group korri-sunshine-input-seat >/dev/null; then
      test "$(@getent@/bin/getent group korri-sunshine-input-seat | @coreutils@/bin/cut -d: -f3)" = 980
    else
      @groupadd@/bin/groupadd --system --gid 980 korri-sunshine-input-seat
    fi
    @coreutils@/bin/install -d -m 0700 -o korri -g korri /home/korri/.config/sunshine
    @coreutils@/bin/mkdir -p /run/udev/rules.d
    @coreutils@/bin/install -m 0644 @rules@ "$rules"
    @udevadm@/bin/udevadm control --reload
    @udevadm@/bin/udevadm trigger --action=change --subsystem-match=misc
    @udevadm@/bin/udevadm trigger --action=change --subsystem-match=input
    ;;
  stop)
    if @getent@/bin/getent group korri-sunshine-input-seat >/dev/null; then
      test "$(@getent@/bin/getent group korri-sunshine-input-seat | @coreutils@/bin/cut -d: -f3)" = 980
    else
      echo "Sunshine input-seat group is missing" >&2
      exit 1
    fi
    @coreutils@/bin/rm -f "$rules"
    @udevadm@/bin/udevadm control --reload
    @udevadm@/bin/udevadm trigger --action=change --subsystem-match=misc
    @udevadm@/bin/udevadm trigger --action=change --subsystem-match=input
    @groupdel@/bin/groupdel korri-sunshine-input-seat
    ;;
  *)
    echo "usage: korri-sunshine-input-seat-setup <start|stop>" >&2
    exit 2
    ;;
esac
