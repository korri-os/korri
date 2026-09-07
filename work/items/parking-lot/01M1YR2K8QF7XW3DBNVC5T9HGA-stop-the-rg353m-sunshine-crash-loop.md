---
id: 01M1YR2K8QF7XW3DBNVC5T9HGA
slug: stop-the-rg353m-sunshine-crash-loop
title: Stop the RG353M Sunshine crash loop
origin: parked
status: To Do
priority: high
labels:
  - rg353m
  - sunshine
  - streaming
created: 2026-09-07
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  repo: korri
---

# Stop the RG353M Sunshine crash loop

## Why it matters

`sunshine.service` on the RG353M restarts continuously with `signal=SEGV`. The
unit rarely reaches a settled `active` state, so `systemctl is-active` usually
reports `activating`. Streaming cannot be relied on while this continues.

The encoder itself is not the fault. Each attempt reaches
`Trying encoder [rkmpp]`, then `Creating encoder [h264_rkmpp]`, enumerates the
KMS monitor list, and finds `DSI-1` at 640x480. The crash happens after that.

## Evidence

Measured on 2026-09-07, all on the device:

| Window | SEGV count |
| --- | ---: |
| Boot running the pre-rebase system, before any change today | 466 |
| Boot where the compositor never started, so Sunshine never ran | 0 |
| Current boot, 20 minutes before the RKVENC rebase was activated | 114 |
| Current boot, 5 minutes after that activation | 8 |

The defect predates the RKVENC rebase and the KMS device fix. Neither change
caused it and neither made it worse. The zero-count boot only shows that
Sunshine does not run when the compositor is down.

The portal is unaffected: the kiosk, nginx, korrid, and inputd stay active and
the portal answers HTTP 200 throughout.

## Acceptance Criteria

- Capture a backtrace. `systemd-coredump` is the least invasive route, or run
  the wrapped binary under `gdb` on the device.
- Name the faulting call. State whether it sits in Sunshine, in the RKMPP
  encoder path, or in the KMS capture path.
- `sunshine.service` reaches `active` and stays there for 10 minutes with zero
  restarts.
- A real Moonlight session runs for 60 seconds without a restart.

## Notes

The RKMPP acceptance record `docs/acceptance/sunshine-korri-rg353m-rkmpp-2026-09-05.md`
reports stable soaks of 40 and 180 seconds under `switch-to-configuration test`.
Compare that unit configuration against the one shipping now; the difference may
identify the regression.
