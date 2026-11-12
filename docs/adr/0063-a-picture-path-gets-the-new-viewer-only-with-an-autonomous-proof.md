---
status: accepted
---

# A picture path gets the new viewer only with an autonomous proof

**Owner decisions, 2026-11-11.** ADR 0062 says that agents run all validation
of the new viewer without the owner. This ADR says where that proof comes
from, and what happens to a device that has none.

## Decision

- aka runs the sender and a test viewer together for every change. That run
  proves the session part and the network library. A viewer on aka is a test
  fixture. The test starts it and removes it, and aka stays a server-only
  device (ADR 0051).
- The sender of that run is also a fixture: a private compositor and a private
  Sunshine on their own ports, beside the product. The run does not use aka's
  display or the product Sunshine, so a live game does not stop it. The owner
  watched one run during a game on 2026-11-12 and noticed nothing. A result
  records whether a game was live, and a verdict does not compare the two
  kinds.
- No fixture runs on a board with a device tree. Such a device is taken for a
  passive handheld and runs only its installed viewer. Each agent also reads
  the device's temperature, does not start on a hot device and ends a run near
  the device's own limit. An RG353M reset during a test before these rules
  existed.
- A device bench proves each picture path. The bench is the RG353M (RK3566),
  the RG35XX Pro (H700) and a Galaxy Z Fold3 for Android. The RG35XX SP has the
  Pro's chip and is not measured separately. Bench devices stay powered and
  reachable, and agents restart them at any time. An Android emulator also
  runs for every change.
- A device with no proof is postponed and keeps the old Moonlight viewer. The
  Qualcomm devices (RP Mini V2, Odin 2 Portal) have no bench device, and the
  owner has not decided to keep them. The R36T Max and the RG DS use the
  software fallback. aka proves that code but not its speed on their chips.
  A postponed device moves after the owner docks it for a proof run.
- The owner validates by hand once for each stage and platform, after the
  autonomous proof passes.

## Why

A viewer's performance is decided in its picture path, and each picture path
exists only on its own kind of device. aka cannot prove a handheld's or a
phone's decoder. Without a bench, the owner's hand check would be the first
test of a path, which the owner named as failure.

## Consequences

- Two viewers exist on Linux while a device is postponed.
- A stage is done for a device only when that device's path has a proof.
