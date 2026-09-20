# Migrate RG DS to the product module

Status: ready-for-agent
Blocked by: 01

## What to build

Make RG DS run the shared Korri product while keeping only its real board facts and explicit optional limits in the device module.

## Acceptance criteria

- [ ] RG DS imports the product module from ticket 01.
- [ ] Its device module contains hardware facts and recorded limits only. It does not add, remove, replace, or force-disable a product service.
- [ ] The runtime account comes only from the product module as `korri` uid and gid 1000 with home `/home/korri`.
- [ ] Existing display, render, rotation, input, kernel, device-tree, and boot facts are preserved as hardware facts without renaming or redesigning them.
- [ ] Device-owned product toggles are removed. Any missing required behavior keeps the image in development and is not relabeled as a hardware limit.
- [ ] The RG DS configuration passes the product check with no device exception.
- [ ] Focused device evaluation checks cover its hardware facts. They do not duplicate product rules.
- [ ] The migration adds no account migration, alias, fallback read, dual write, or compatibility branch.
- [ ] A passing evaluation is not recorded as physical support.
