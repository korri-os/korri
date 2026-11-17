#!/usr/bin/env python3
"""Diagnostic only (#391): each TCP connection of a capture.sh capture, from `tcpdump -r CAP -tt -nn -A`
on standard input. One row per connection: when the client's SYN, the server's SYN-ACK, the client's
first data (its request) and the server's first data (its answer) passed on aka's loopback, and the
request's first words. Times in ms from the first SYN."""
import re
import sys

HEAD = re.compile(r"^(\d+\.\d+) .*?IP (\S+)\.(\d+) > (\S+)\.(\d+): Flags \[([^\]]*)\].*length (\d+)")
WORDS = re.compile(r"(OPTIONS|DESCRIBE|SETUP|ANNOUNCE|PLAY|GET) (\S+)")

server_ports = {int(port) for port in sys.argv[1:]}
connections: dict = {}
order: list = []
current = None
for line in sys.stdin:
    found = HEAD.match(line)
    if not found:
        if current is not None and current.get("words") is None:
            words = WORDS.search(line)
            if words:
                current["words"] = f"{words.group(1)} {words.group(2)[:60]}"
        continue
    at, source, sport, target, dport, flags, length = found.groups()
    at, sport, dport, length = float(at), int(sport), int(dport), int(length)
    client_side = dport in server_ports
    key = (sport, dport) if client_side else (dport, sport)
    row = connections.get(key)
    if row is None:
        row = connections[key] = {"client_port": key[0], "server_port": key[1], "words": None}
        order.append(key)
    current = row if (client_side and length > 0) else None
    if client_side and "S" in flags:
        row.setdefault("syn", at)
    elif not client_side and "S" in flags:
        row.setdefault("syn_ack", at)
    elif client_side and length > 0:
        row.setdefault("request", at)
    elif not client_side and length > 0:
        row.setdefault("answer", at)
    if "F" in flags:
        row.setdefault("fin_client" if client_side else "fin_server", at)

print("port  syn_epoch  synack+  request+  answer+  server_fin+  words")
for key in order:
    row = connections[key]
    if "syn" not in row:
        continue
    def after(name, base="syn"):
        return f"{(row[name] - row[base]) * 1e3:8.1f}" if name in row and base in row else "       -"
    print(f"{row['server_port']} {row['syn']:.6f} {after('syn_ack')} {after('request')} "
          f"{after('answer')} {after('fin_server')}  {row['words'] or ''}")
