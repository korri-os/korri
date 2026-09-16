---
id: 01M289ADPZBMEG50A7DEPKXX5Y
slug: validate-rg353m-ipv4-with-simultaneous-ethernet-and-wi-fi
title: Validate RG353M IPv4 with simultaneous Ethernet and Wi-Fi
origin: parked
status: To Do
priority: medium
labels:
  - rg353m
  - networking
  - boot-validation
created: 2026-09-11
source: user
---

# Validate RG353M IPv4 with simultaneous Ethernet and Wi-Fi

## Why it matters

The updated candidate associates over Wi-Fi and carries IPv6 gateway traffic during Bluetooth discovery, including after resume. Interface-bound IPv4 gateway pings fail while Ethernet works. Both interfaces share a subnet, the preferred return route uses Ethernet, and the strict nixos-fw-rpfilter chain records drops. Separate multihoming/filter behavior from a radio problem before changing policy; do not silently weaken the firewall.

## Acceptance Criteria

- [x] Use bounded packet observation or equivalent evidence to identify the failed IPv4 return path.
- [ ] Verify Wi-Fi IPv4 with Ethernet disconnected under a recoverable test plan, then verify simultaneous-interface behavior.
- [ ] Review any routing or firewall policy change explicitly; retain SSH and packet-filter protections.
- [ ] Verify Wi-Fi/Bluetooth coexistence after any justified fix.

## Related

- `nix/devices/rg353m/wifi.nix`
- `nix/base/default.nix`

## Notes

New system qgxaaslyj016y4v0f9g2pn7wcgz9jrfg. IPv6 link-local gateway pings scoped to wlan0 passed 4/4 during Bluetooth discovery before and after timed suspend. Bluetooth found two nearby devices. IPv4 bound pings lost all replies with Bluetooth idle as well. Ethernet gateway pings passed. Kernel rp_filter=2, but iptables mangle PREROUTING has strict --validmark rpfilter. Drop counter rose 1086 to 1093 during a two-ping test; other traffic prevents assigning every drop to those probes.

Follow-up on phase-2 generation ijnr12p64qmsafnrgzl0slrmz419jm3b: a bounded tcpdump capture restricted to three test ICMP exchanges showed all three requests leaving wlan0 and matching replies arriving on enu1. Ping was bound to wlan0 and received none. This demonstrates asymmetric reply delivery; earlier filter counters do not prove those replies were dropped by rpfilter. ARP ignore/filter/announce are all zero on both interfaces and globally, consistent with weak-host/ARP-flux behavior on a shared subnet. No routes, interface state, ARP settings or firewall policy were changed. The capture printed packet headers only, not other traffic or payloads.
