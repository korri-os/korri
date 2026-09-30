# Package choices recorded in
# .scratch/consistent-korri-product/evidence/rocknix-recommendations.md.
# Simon removed PSP, DraStic, YabaSanshiro, Dolphin, Azahar and AetherSX2
# from this release's defaults. These are build-time choices, not a persisted
# plugin schema or proof that a device can run each selected game.
{ plugins }:
let
  select = names: map (name: plugins."korri-plugin-${name}") names;
  shared = select [
    "retroarch"
    "nestopia"
    "snes9x"
    "gambatte"
    "mgba"
    "genesis-plus-gx"
    "picodrive"
    "beetle-pce-fast"
    "stella"
    "prosystem"
    "handy"
    "beetle-ngp"
    "beetle-wswan"
    "gw"
    "fbneo"
    "pcsx-rearmed"
  ];
  n64AndDreamcast = select [ "mupen64plus" "flycast" ];
  ds = select [ "melonds" ];
  streamingHost = [ plugins.korri-plugin-sunshine ];
  # Key-only SSH on every device (owner decision, 2026-09-30): it is the copy
  # and debug path over the USB network link and Wi-Fi. Root authorization
  # remains device-owned; with no authorized key nobody can log in.
  ssh = [ plugins.korri-plugin-ssh ];
in
{
  rg353m = shared ++ n64AndDreamcast ++ streamingHost ++ ssh;
  rgds = shared ++ n64AndDreamcast ++ streamingHost ++ ssh;
  r36tmax = shared ++ ssh;
  rpminiv2 = shared ++ n64AndDreamcast ++ ds ++ streamingHost ++ ssh;
  odin2portal = shared ++ n64AndDreamcast ++ ds ++ streamingHost ++ ssh;
  # RG35XXSP is not a product image yet. Its choice becomes active only when
  # measured display/input facts allow the product module and image to land.
  rg35xxsp = shared ++ ssh;
}
