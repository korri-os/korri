# Marker dry-run output — not flashable

This output proves that Korri can make one controlled stock-filesystem change and rebuild the Odin 2 Portal dynamic partition container without contacting a device.

The only intended filesystem-content change is one ignored comment appended to the existing product `/etc/build.prop`:

```text
# korri marker-only dry run
```

The product logical partition grows by 16 MiB. This growth is grounded by the stock image: its shared-block ext4 filesystem has only 708 free 4 KiB blocks, and a measured unshare attempt exhausts them. A 16 MiB partition increase provides enough space to unshare the deduplicated blocks and retain validation headroom.

## Why this output must not be flashed

AYN's `vbmeta_system` uses `SHA256_RSA2048`. Its private signing key is unavailable. This dry run therefore emits an unsigned `vbmeta_system_a.img` with `Algorithm: NONE` and regenerates the changed product hashtree without FEC because the required Android `fec` host tool is not available in the pinned Nix toolchain.

The stock root `vbmeta` chain still expects AYN's signed `vbmeta_system` key. The generated verification chain is internally consistent on the host but is not accepted by that stock chain.

To prevent accidental use, partition-shaped outputs are quarantined under `NON_FLASHABLE_ARTIFACTS/` and carry the `.not-flashable` suffix. A later flash tool must reject these names.

A later flash slice must explicitly choose, document, and test an unlocked-device AVB strategy. This output does not make that choice. It contains no flashing command and performs no device write.
