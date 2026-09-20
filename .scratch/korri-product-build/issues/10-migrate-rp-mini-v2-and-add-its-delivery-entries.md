# Migrate RP Mini V2 and add its delivery entries

Status: ready-for-agent
Blocked by: 01

## What to build

Make RP Mini V2 run the shared Korri product and publish it through the normal image and signed-cache paths.

## Acceptance criteria

- [ ] RP Mini V2 imports the product module from ticket 01.
- [ ] Its device module contains hardware facts and recorded limits only. It does not add, remove, replace, or force-disable a product service.
- [ ] The hand-disabled Sunshine service and certificate-control socket are removed. Streaming-host selection is not represented by disabled shared units.
- [ ] The runtime account comes only from the product module as `korri` uid and gid 1000 with home `/home/korri`.
- [ ] Existing display, OLED, render, input, kernel, device-tree, and boot facts are preserved as hardware facts without a new schema.
- [ ] The image workflow publishes an RP Mini V2 installation image, and the cache workflow publishes its complete closure from the same commit with signature checks intact.
- [ ] The RP Mini V2 configuration passes the product check with no device exception, including the delivery assertions.
- [ ] Focused device and workflow checks cover the migration and delivery entries.
- [ ] The migration adds no account migration, alias, fallback read, dual write, or compatibility branch.
- [ ] Delivery and evaluation do not by themselves grant supported-image status.
