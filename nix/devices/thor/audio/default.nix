# ALSA UCM2 policy for the AYN Thor sound card, layered onto alsa-ucm-conf.
#
# The kernel exposes the card as `AYNThor` (DTS model = "AYN-Thor"): WCD938x
# codec plus two aw88166 speaker amplifiers. WirePlumber looks the card up in
# UCM; without a matching profile it offers only `pro-audio` and creates no
# speaker or headphone sink, so the device is silent.
#
# The two .conf files under ucm2/AYN/Thor/ are Luke Johnson's Thor profile,
# carried unchanged from ROCKNIX distribution rev
# f080b462f54b5807bdd16ac7cc2ab64528b038b1,
# projects/ROCKNIX/packages/audio/alsa-ucm-conf/patches/SM8550/0005_Add-AYN-Thor.patch.
# It is the Odin 2 profile plus the Thor's internal microphone route (WCD938x
# DMIC3 through SWR_MIC7).
#
# UCM2 finds a card's profile at conf.d/<driver>/<longname>.conf. On the Thor
# the card reports driver `sm8550` and longname `AYN-Thor`
# (api.alsa.card.longname, 2026-11-19), so the one entry is
# conf.d/sm8550/AYN-Thor.conf. ROCKNIX also ships `ayn-AYNThor-.conf` with a
# trailing dash for its own boot path; it does not match here and is left out.
#
# Verified on device 2026-11-19 with this tree passed as ALSA_CONFIG_UCM2:
# verb `HiFi` lists Speaker, Headphones, Mic and DisplayPort, and WirePlumber
# creates `alsa_output.platform-sound.HiFi__Speaker__sink`.
{ alsa-ucm-conf, stdenvNoCC }:

stdenvNoCC.mkDerivation {
  pname = "ayn-thor-ucm";
  version = "2026-05-09";

  dontUnpack = true;

  installPhase = ''
    runHook preInstall

    mkdir -p $out/share/alsa/ucm2
    cp -a ${alsa-ucm-conf}/share/alsa/ucm2/. $out/share/alsa/ucm2/
    chmod -R u+w $out/share/alsa/ucm2
    cp -a ${./ucm2}/. $out/share/alsa/ucm2/

    runHook postInstall
  '';

  meta.description = "AYN Thor ALSA UCM2 policy layered onto alsa-ucm-conf";
}
