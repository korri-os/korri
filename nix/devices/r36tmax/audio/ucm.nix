# Data-only device package; do not override alsa-lib or its dependency graph.
# Patch copied unchanged from ROCKNIX/distribution@697d112a64e442e79e3d27b064351d917657301a:
# projects/ROCKNIX/packages/audio/alsa-ucm-conf/patches/RK3326/0001_fix_speaker_when_booting_with_headphones.patch
{ alsa-ucm-conf }:
alsa-ucm-conf.overrideAttrs (previous: {
  patches = (previous.patches or [ ]) ++ [ ./0001_fix_speaker_when_booting_with_headphones.patch ];
})
