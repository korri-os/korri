#!/usr/bin/env bash
# Diagnostic only (#391, not in the repository): once a play proof's sender
# fixture is up, capture the TCP headers (and the first 160 bytes) of its
# Sunshine's HTTP, HTTPS and RTSP ports on aka's loopback, where the emulator's
# connections to aka's own address arrive. Stops when the proof writes result.json.
set -euo pipefail
out=$1
pcap=$2
until [ -s "$out/sender-facts.json" ]; do [ -e "$out/result.json" ] && exit 0; sleep 0.5; done
base=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sunshine_port"])' "$out/sender-facts.json")
echo "sunshine http port $base" >&2
tcpdump -i any -n -s 160 -U -w "$pcap" "tcp and (port $((base - 5)) or port $base or port $((base + 21)))" &
dump=$!
until [ -e "$out/result.json" ]; do sleep 1; done
sleep 1
kill -INT "$dump"
wait "$dump" || true
