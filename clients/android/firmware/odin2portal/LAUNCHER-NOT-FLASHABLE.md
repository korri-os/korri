# Korri launcher image is quarantined

This host output adds the signed Korri release APK at `/product/app/Korri/Korri.apk`, retains the stock AYN launcher and Android Settings, and preserves the AYN boot and recovery chain keys. It also retains the exact marker comment used by the proven first mutation.

Partition-shaped files stay under `NON_FLASHABLE_ARTIFACTS/` and use `.not-flashable` suffixes. Host verification does not approve a device write. Installation needs a new artifact contract, a reviewed rollback plan, live device gates, and separate explicit approval.

Korri is only a HOME candidate in this image. The image does not write the preferred HOME for the existing user. Read `HOME-PROVISIONING.md`. That package-manager state change is separate and must keep `com.odin.odinlauncher` available as fallback.

The bootloader must remain unlocked. The orange startup warning and reduced protection from physical access are expected. The product hashtree has no FEC. Do not lock or relock the bootloader.
