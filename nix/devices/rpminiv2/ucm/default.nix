# ALSA UCM for the Retroid Pocket SM8250 card ("RetroidPocket").
#
# The two patches are ROCKNIX distribution e81d1fc943458fb13cffe1646761e9452b29ddc1
# projects/ROCKNIX/packages/audio/alsa-ucm-conf/patches/SM8250/, unchanged:
#   0002 adds the HiFi verb (headphones, speakers, headset mic), speaker and
#        headphone boot volumes, and the WSA881x stereo volume map.
#   0004 adds the separate DisplayPort verb.
# ROCKNIX's 0003 only adds names for an older loader and is not used.
#
# alsa-lib looks for conf.d/<driver>/<long name>.conf. The kernel builds the
# long name from DMI with spaces removed; the Mini V2 reports vendor
# "retroidpocket" and product "Retroid Pocket Mini V2" (journal, 2026-09-25).
# An empty product version may add a trailing "-", so both forms are linked.
{ alsa-ucm-conf }:
alsa-ucm-conf.overrideAttrs (old: {
  pname = "alsa-ucm-conf-rpminiv2";
  patches = (old.patches or [ ]) ++ [
    ./0002_Add-Retroid-Pocket-SM8250-configuration.patch
    ./0004_Add-HDMI-DisplayPort-audio-device.patch
  ];
  postInstall = (old.postInstall or "") + ''
    conf=$out/share/alsa/ucm2/conf.d/sm8250
    test -f $out/share/alsa/ucm2/Qualcomm/sm8250/RetroidPocket.conf
    for name in retroidpocket-RetroidPocketMiniV2 retroidpocket-RetroidPocketMiniV2-; do
      ln -sfn ../../Qualcomm/sm8250/RetroidPocket.conf "$conf/$name.conf"
    done
    test -f "$conf/retroidpocket-RetroidPocketMiniV2-.conf"
  '';
})
