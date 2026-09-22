# Migrate R36T Max to the product module

Status: resolved
Blocked by: 01

## What to build

Make R36T Max run the shared Korri product and state its real hardware limits plainly, without hand-disabled product units.

## Acceptance criteria

- [ ] R36T Max imports the product module from ticket 01.
- [ ] Its device module contains hardware facts and recorded limits only. It does not add, remove, replace, or force-disable a product service.
- [ ] The runtime account comes only from the product module as `korri` uid and gid 1000 with home `/home/korri`.
- [ ] Existing display, render, input, kernel, device-tree, and boot facts are preserved as hardware facts without a new schema.
- [ ] The missing H.264 encoder is recorded as a limit. No disabled streaming-host unit is used to represent that limit.
- [ ] Device-owned product toggles are removed. Any missing required behavior keeps the image in development and is not relabeled as a hardware limit.
- [ ] The R36T Max configuration passes the product check with no device exception.
- [ ] Focused device evaluation checks cover its hardware facts. They do not duplicate product rules.
- [ ] The migration adds no account migration, alias, fallback read, dual write, or compatibility branch.
- [ ] The existing distribution hold is not silently removed, and a passing evaluation is not recorded as physical support.
