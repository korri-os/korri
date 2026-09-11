#!/bin/sh
# EmulationStation launcher stub for the Korri SSH payload.
#
# A copy of this lands in every folder the stock frontend is known to scan
# for launchable scripts, because which folders appear on the menu is
# theme-dependent and differs between stock images. Guessing one name is how
# the May bring-up lost its first hour.
#
# The card's mount point also differs between images, so the stub finds it
# rather than assuming it.
for candidate in \
  /storage/roms \
  /roms \
  /var/media/EEROMS \
  /media/EEROMS \
  /run/media/EEROMS \
  /var/media/KORRI \
  /media/KORRI
do
  if [ -f "$candidate/.korri/install-ssh.sh" ]; then
    sh "$candidate/.korri/install-ssh.sh" "$candidate"
    sync
    exit 0
  fi
done
exit 1
