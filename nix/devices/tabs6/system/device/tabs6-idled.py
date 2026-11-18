"""
tabs6-idled: idle suspend for the Tab S6, ported from the reference port's
tools/tabs6-idled.py (SLEEP.md).

Idleness is read directly from the evdev nodes: if no input device has produced
an event for long enough, nobody is using the tablet. This works with or without
a compositor, which is why it is a system service and not part of sway.

Changes from the port: the KDE inhibitor query is gone (there is no KDE), and
the tablet never suspends while a USB host has the gadget configured, because
suspend drops the USB lifeline (USB_NETWORKING.md section 7). systemd block
inhibitors on sleep or idle are still honoured.
"""

import errno
import glob
import os
import select
import subprocess
import sys
import time

IDLE_ON_AC = int(os.environ.get("TABS6_IDLE_ON_AC", 60 * 60))
IDLE_ON_BATTERY = int(os.environ.get("TABS6_IDLE_ON_BATTERY", 20 * 60))
FAILURE_BACKOFF = 5 * 60
POST_RESUME_GRACE = 60
FUELGAUGE = "/sys/class/power_supply/sm5705-fuelgauge/status"
POLL = 5.0


def log(msg):
    print(msg, flush=True)


def open_input_devices():
    devs = {}
    for path in sorted(glob.glob("/dev/input/event*")):
        try:
            devs[os.open(path, os.O_RDONLY | os.O_NONBLOCK)] = path
        except OSError as e:
            log("skip %s: %s" % (path, e.strerror))
    return devs


def drain(fd):
    while True:
        try:
            if not os.read(fd, 4096):
                return
        except OSError as e:
            if e.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                return
            raise


def on_battery():
    try:
        with open(FUELGAUGE) as f:
            # "Full" and "Not charging" also mean a charger is attached;
            # only "Discharging" is running on the battery.
            return f.read().strip() == "Discharging"
    except OSError:
        return False


def remote_session_active():
    """True while anyone is logged in over SSH: never drop their link."""
    try:
        out = subprocess.run(["loginctl", "list-sessions", "--no-legend"],
                             capture_output=True, text=True, timeout=10)
        sessions = [line.split()[0] for line in out.stdout.splitlines() if line.split()]
    except Exception:
        return False
    for sid in sessions:
        try:
            show = subprocess.run(
                ["loginctl", "show-session", sid, "-p", "Remote", "-p", "State"],
                capture_output=True, text=True, timeout=10).stdout
        except Exception:
            continue
        if "Remote=yes" in show and "State=closing" not in show:
            return True
    return False


def usb_host_attached():
    """True while a host has configured the gadget: keep the lifeline up."""
    for state in glob.glob("/sys/class/udc/*/state"):
        try:
            with open(state) as f:
                if f.read().strip() == "configured":
                    return True
        except OSError:
            pass
    return False


def systemd_block_inhibited():
    try:
        out = subprocess.run(["systemd-inhibit", "--list", "--no-legend"],
                             capture_output=True, text=True, timeout=10)
    except Exception:
        return False
    for line in out.stdout.splitlines():
        if "block" not in line:
            continue
        if any(w.startswith("sleep") or w.startswith("idle") for w in line.split()):
            return True
    return False


def suspend():
    r = subprocess.run(["systemctl", "suspend"], capture_output=True, text=True, timeout=60)
    if r.returncode != 0:
        log("suspend failed: %s%s" % (r.stdout.strip(), r.stderr.strip()))
        return False
    return True


def main():
    devs = open_input_devices()
    if not devs:
        log("no input devices could be opened, nothing to measure - exiting")
        return 1
    log("watching %d input devices" % len(devs))

    last_activity = time.monotonic()
    blocked_until = 0.0

    while True:
        threshold = IDLE_ON_BATTERY if on_battery() else IDLE_ON_AC
        idle = time.monotonic() - last_activity
        wait = POLL if idle + POLL < threshold else max(1.0, threshold - idle)

        try:
            ready, _, _ = select.select(list(devs), [], [], wait)
        except OSError as e:
            log("select failed: %s" % e)
            time.sleep(POLL)
            continue

        if ready:
            for fd in ready:
                try:
                    drain(fd)
                except OSError as e:
                    log("dropping %s: %s" % (devs.get(fd), e.strerror))
                    os.close(fd)
                    devs.pop(fd, None)
            last_activity = time.monotonic()
            continue

        now = time.monotonic()
        if now - last_activity < threshold or now < blocked_until:
            continue

        # Suspend only on battery. On a charger, and while anyone is logged in
        # over SSH, idle only blanks the screen (swayidle).
        keep_awake = not on_battery()
        keep_awake = keep_awake or usb_host_attached()
        keep_awake = keep_awake or remote_session_active()
        keep_awake = keep_awake or systemd_block_inhibited()
        if keep_awake:
            last_activity = now
            continue

        log("idle %.0fs (threshold %ds, %s) - suspending"
            % (now - last_activity, threshold, "battery" if on_battery() else "AC"))
        ok = suspend()
        last_activity = time.monotonic() + (POST_RESUME_GRACE if ok else 0)
        if ok:
            log("resumed")
        else:
            blocked_until = time.monotonic() + FAILURE_BACKOFF


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(0)
