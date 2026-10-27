# Signed AVB dry-run output — not flashable

This output contains a host-verified custom Android Verified Boot chain for the Odin 2 Portal marker image. It does not contain the private signing key.

The custom root `vbmeta` image uses Korri's RSA-4096 key. Its `boot` and `recovery` chain descriptors retain the extracted AYN root public key. Its `vbmeta_system` chain descriptor uses the Korri public key. The signed `vbmeta_system` image covers the modified product hashtree and the unchanged stock system and system-ext hashtrees.

The output is not AYN-trusted. The bootloader must stay unlocked. The device will show the orange startup warning. An unlocked bootloader gives less protection from a person who has physical access. The files have not passed a device boot test. They are stored under `NON_FLASHABLE_ARTIFACTS/` and use the `.not-flashable` suffix.

Do not install these files. A later task must define and approve the exact fastboot states, write order, rollback triggers, and recovery commands.
