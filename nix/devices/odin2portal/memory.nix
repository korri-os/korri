# Compressed RAM swap for the AYN Odin 2 Portal.
#
# The Odin has 7 GiB of RAM and no swap. Native Breath of the Wild (Savage
# Wind) pins about 5 GiB as unevictable GPU-shared memory, so everything else
# shares about 2 GiB. With no swap, available memory fell to 55-75 MiB and the
# game dropped to about 2 FPS while the kernel evicted code pages. A 2 GiB
# zram device added at runtime on 2026-10-31 raised available memory to about
# 280 MiB and restored 52 FPS. That device was lost at each reboot.
#
# zram lives only in RAM. It writes nothing to internal storage or the SD card.
{
  zramSwap.enable = true;
}
