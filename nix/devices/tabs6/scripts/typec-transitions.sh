#!/usr/bin/env nix-shell
#!nix-shell -i bash -p systemd gnugrep gnused gawk coreutils
# Read-only: collapse the Type-C observer's journal into state transitions
# (only lines where the decoded fields change), with the MUIC/charger fields.
#   ssh -p 2222 root@tabs6.local bash -s < nix/devices/tabs6/scripts/typec-transitions.sh
export PATH=/run/current-system/sw/bin:$PATH
journalctl -b -u tabs6-typec-status -o cat --no-pager 2>/dev/null |
	grep '^uptime=' |
	sed -E 's/^uptime=([0-9.]+) //; s/ irq=[0-9a-f]+//; s/ lp=[0-9a-f]+//' |
	awk '{t=$0; key=t; sub(/^[^ ]+ /,"",key)} key!=last {print; last=key}' |
	tail -60
