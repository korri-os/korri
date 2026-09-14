#!/bin/sh
# ROCKNIX's /usr/bin/autostart runs every file in /storage/.config/autostart/
# as root at every boot, with no input from anyone. On a board with no
# keyboard, no network, and no USB data lines, that is the only hand that can
# type, so it does everything in one boot.
#
# Two jobs.
#
# 1. Get our kernel's crash log off this device. Our image panics a second
#    into boot and writes its console to the ramoops region at 0x110000.
#    This ROCKNIX card reserves the same region, so on boot the records are
#    either still in /sys/fs/pstore or already moved to
#    /var/lib/systemd/pstore. Both are copied.
#
# 2. Record everything about a kernel that works on this board, from the
#    board itself: what the device tree resolved to, which drivers bound,
#    what the regulators and clocks settled at, what the USB controller
#    thinks about its cable. Every one of these is a question that would
#    otherwise cost a boot each.
#
# Everything lands on the FAT boot partition, which the card reader can
# read. README.txt at the top says what succeeded, so an empty directory
# still explains itself.
stamp=$(date +%Y%m%d-%H%M%S)
out=/flash/korri/$stamp
readme=$out/README.txt

mount -o remount,rw /flash 2>/dev/null
mkdir -p "$out" || exit 0
# The regulator, clock, gpio, and DRM dumps live in debugfs. ROCKNIX has a
# unit for it, but do not depend on ordering.
mountpoint -q /sys/kernel/debug || mount -t debugfs none /sys/kernel/debug 2>/dev/null
# pstore is a filesystem and needs mounting to be read.
mountpoint -q /sys/fs/pstore || mount -t pstore pstore /sys/fs/pstore 2>/dev/null

say() { echo "$*" >> "$readme"; }
# Run a command, save its output, note the outcome. Never stop on failure:
# a missing tool is itself an answer.
grab() {
  name=$1; shift
  if "$@" > "$out/$name" 2>&1; then say "ok    $name"; else say "FAIL  $name ($*)"; fi
}

say "=== korri survey $stamp ==="
say "kernel: $(uname -r)"
say "model:  $(tr -d '\0' < /proc/device-tree/model 2>/dev/null)"
say "up:     $(cut -d' ' -f1 /proc/uptime)s"
say ""

# --- 1. pstore ------------------------------------------------------------
say "--- pstore ---"
mkdir -p "$out/pstore-live" "$out/pstore-archived"
grab pstore-mount.txt sh -c 'mount | grep pstore'
grab pstore-dmesg.txt sh -c 'dmesg | grep -iE "ramoops|pstore"'
grab pstore-reserved.txt ls -la /proc/device-tree/reserved-memory/
grab pstore-live-ls.txt ls -la /sys/fs/pstore/
cp -a /sys/fs/pstore/. "$out/pstore-live/" 2>/dev/null
cp -a /var/lib/systemd/pstore/. "$out/pstore-archived/" 2>/dev/null
say "live records:     $(find "$out/pstore-live" -type f | wc -l)"
say "archived records: $(find "$out/pstore-archived" -type f | wc -l)"
say ""

# --- 2. the working kernel, described by itself ---------------------------
say "--- kernel and boot ---"
grab dmesg.txt dmesg
grab cmdline.txt cat /proc/cmdline
grab version.txt cat /proc/version
grab config.gz cat /proc/config.gz
grab modules.txt cat /proc/modules
grab iomem.txt cat /proc/iomem
grab interrupts.txt cat /proc/interrupts
grab cpuinfo.txt cat /proc/cpuinfo
grab meminfo.txt cat /proc/meminfo
grab boot-log.txt cat /var/log/boot.log

say ""
say "--- device tree as the kernel resolved it ---"
# The whole live tree, so the exact node names, phandles, and statuses that
# a working boot used can be diffed against ours. ROCKNIX ships no dtc, so
# copy the raw tree; the kernel also exposes it as one flattened blob at
# /sys/firmware/fdt, which dtc on the host decompiles in one step.
grab fdt.dtb cat /sys/firmware/fdt
cp -a /proc/device-tree "$out/device-tree" 2>/dev/null && say "ok    device-tree/ (raw)"
grab extlinux.conf cat /flash/extlinux/extlinux.conf

say ""
say "--- drivers that bound, and to what ---"
grab drivers-bound.txt sh -c 'for d in /sys/bus/platform/drivers/*/; do n=$(basename "$d"); for b in "$d"*; do case $(basename "$b") in bind|unbind|uevent|module) continue;; esac; [ -L "$b" ] && echo "$n  ->  $(basename "$b")"; done; done'
grab platform-devices.txt ls /sys/bus/platform/devices/
grab i2c.txt sh -c 'for d in /sys/bus/i2c/devices/*; do echo "$(basename $d): $(cat $d/name 2>/dev/null) driver=$(basename $(readlink $d/driver 2>/dev/null) 2>/dev/null)"; done'
grab mmc.txt sh -c 'for h in /sys/class/mmc_host/*; do echo "== $(basename $h)"; for c in $h/mmc*; do [ -d "$c" ] || continue; echo "  $(basename $c): type=$(cat $c/type 2>/dev/null) name=$(cat $c/name 2>/dev/null)"; done; done; echo; cat /proc/partitions'
grab block.txt sh -c 'cat /proc/partitions; echo; cat /proc/mounts'

say ""
say "--- power ---"
grab regulators.txt sh -c 'for r in /sys/class/regulator/*; do echo "$(cat $r/name 2>/dev/null): state=$(cat $r/state 2>/dev/null) uV=$(cat $r/microvolts 2>/dev/null) min=$(cat $r/min_microvolts 2>/dev/null) max=$(cat $r/max_microvolts 2>/dev/null) users=$(cat $r/num_users 2>/dev/null)"; done'
grab regulator-summary.txt cat /sys/kernel/debug/regulator/regulator_summary
grab clk-summary.txt cat /sys/kernel/debug/clk/clk_summary
grab power-supply.txt sh -c 'for p in /sys/class/power_supply/*; do echo "== $(basename $p)"; grep -H . $p/uevent 2>/dev/null; done'
grab thermal.txt sh -c 'for t in /sys/class/thermal/thermal_zone*; do echo "$(cat $t/type): $(cat $t/temp)"; done'

say ""
say "--- pins and gpio ---"
grab gpio.txt cat /sys/kernel/debug/gpio
grab pinctrl-maps.txt cat /sys/kernel/debug/pinctrl/pinctrl-maps
grab pinctrl-pins.txt sh -c 'cat /sys/kernel/debug/pinctrl/*/pins'
grab pinmux.txt sh -c 'cat /sys/kernel/debug/pinctrl/*/pinmux-pins'

say ""
say "--- display ---"
grab drm-summary.txt cat /sys/kernel/debug/dri/0/summary
grab drm-state.txt cat /sys/kernel/debug/dri/0/state
grab drm-connectors.txt sh -c 'for c in /sys/class/drm/card*-*; do echo "$(basename $c): status=$(cat $c/status) enabled=$(cat $c/enabled) modes=$(cat $c/modes | tr "\n" " ")"; done'
grab backlight.txt sh -c 'for b in /sys/class/backlight/*; do echo "$(basename $b): brightness=$(cat $b/brightness) max=$(cat $b/max_brightness) power=$(cat $b/bl_power)"; done'
grab fb.txt sh -c 'cat /sys/class/graphics/fb0/virtual_size; cat /sys/class/graphics/fb0/modes'

say ""
say "--- usb: does this port have data lines? ---"
# The controller says what it sees. A cable with data lines shows as SDP or
# CDP with state=1 on the phy's extcon; a charge-only cable or no data pins
# shows nothing there at all.
grab usb-udc.txt sh -c 'for u in /sys/class/udc/*; do echo "$(basename $u): state=$(cat $u/state) function=$(cat $u/function 2>/dev/null) speed=$(cat $u/current_speed 2>/dev/null) soft_connect=$(cat $u/soft_connect 2>/dev/null)"; done'
grab usb-extcon.txt sh -c 'for e in /sys/class/extcon/*; do echo "== $(basename $e) $(cat $e/name)"; for c in $e/cable.*; do echo "  $(cat $c/name)=$(cat $c/state)"; done; done'
grab usb-gadget-configfs.txt sh -c 'find /sys/kernel/config/usb_gadget -maxdepth 3 2>/dev/null; for g in /sys/kernel/config/usb_gadget/*/; do echo "== $g UDC=[$(cat $g/UDC 2>/dev/null)]"; done'
grab usb-gadget-state.txt sh -c 'cat /storage/.cache/usbgadget/*.conf 2>/dev/null; echo; ip -o link; echo; ip -4 -o addr'
grab usb-dwc2-debug.txt sh -c 'for d in /sys/kernel/debug/usb/*/; do echo "== $d"; cat $d/state $d/status 2>/dev/null; done'
grab usb-dr-mode.txt sh -c 'for n in /proc/device-tree/usb@*; do echo "$(basename $n): dr_mode=$(tr -d "\0" < $n/dr_mode 2>/dev/null) status=$(tr -d "\0" < $n/status 2>/dev/null)"; done'
grab usb-dmesg.txt sh -c 'dmesg | grep -iE "dwc2|usb|udc|gadget|phy"'

say ""
say "--- input ---"
grab input-devices.txt cat /proc/bus/input/devices
grab joypad-dt.txt sh -c 'for n in /proc/device-tree/joypad* /proc/device-tree/adc-joystick* /proc/device-tree/gpio-keys*; do [ -e "$n" ] || continue; echo "== $n"; find "$n" -type f | while read f; do printf "%s = " "${f#$n/}"; tr -d "\0" < "$f" | head -c 200; echo; done; done'
grab iio.txt sh -c 'for d in /sys/bus/iio/devices/*; do echo "== $(basename $d) $(cat $d/name)"; for c in $d/in_voltage*_raw; do echo "  $(basename $c)=$(cat $c 2>/dev/null)"; done; done'

say ""
say "--- audio ---"
grab alsa-cards.txt cat /proc/asound/cards
grab amixer.txt amixer contents
grab audio-dmesg.txt sh -c 'dmesg | grep -iE "rk817|codec|simple-audio|i2s|sound"'

say ""
say "--- wifi: does the vendor module have a home here? ---"
grab wifi.txt sh -c 'ls /sys/class/net; echo; ls /lib/firmware | grep -i rk9; echo; dmesg | grep -iE "sdio|mmc2|rk915|wifi|wlan|brcm|rtl8"'

say ""
say "--- rocknix identity ---"
grab os-release.txt cat /etc/os-release
grab quirk-device.txt sh -c 'echo "HW_DEVICE=$HW_DEVICE QUIRK_DEVICE=$QUIRK_DEVICE"; ls /usr/lib/autostart/quirks/devices/ 2>/dev/null'
grab storage-config.txt ls -la /storage/.config/ /storage/.config/autostart/

say ""
say "=== done $(date) ==="
sync
