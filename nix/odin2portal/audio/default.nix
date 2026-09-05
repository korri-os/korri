# ALSA UCM2 policy for the AYN Odin 2 sound card, layered onto alsa-ucm-conf.
#
# The kernel exposes the card as `AYNOdin2` (from the qcs8550-ayn-common
# sound node, model = "AYN-Odin2"). WirePlumber looks the card up in UCM by
# that name; without a matching profile it creates no speaker or headphone
# sink and the card is silent. The two .conf files under ucm2/AYN/Odin2/ are
# Teguh Sobirin's use-case definitions for this codec (WCD938x +
# aw88166 amplifiers), carried unchanged from nix-on-rocks
# devices/sm8550/audio/ayn-odin2-ucm, revision 2026-05-11.
#
# UCM2 finds a card's profile at conf.d/<driver>/<longname>.conf. This card
# reports driver `sm8550` and longname `ayn-AYNOdin2` (second line of
# /proc/asound/cards), so the entry is conf.d/sm8550/ayn-AYNOdin2.conf. The
# nix-on-rocks copy named it `ayn-AYNOdin2-.conf` with a trailing dash, which
# never matched; the guest had opened the card by an explicit alsaucm -c
# name and did not go through the lookup. The Thor aliases are dropped
# because this package serves one board.
#
# The package is the whole ucm2 tree so ALSA_CONFIG_UCM2 can point at one
# directory: stock profiles for everything else, ours for this card.
{ alsa-ucm-conf, stdenvNoCC }:

stdenvNoCC.mkDerivation {
  pname = "ayn-odin2-ucm";
  version = "2026-05-11";

  dontUnpack = true;

  installPhase = ''
    runHook preInstall

    mkdir -p $out/share/alsa/ucm2
    cp -a ${alsa-ucm-conf}/share/alsa/ucm2/. $out/share/alsa/ucm2/
    chmod -R u+w $out/share/alsa/ucm2
    cp -a ${./ucm2}/. $out/share/alsa/ucm2/

    runHook postInstall
  '';

  meta.description = "AYN Odin 2 ALSA UCM2 policy layered onto alsa-ucm-conf";
}
