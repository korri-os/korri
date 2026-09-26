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
in
{
  rg353m = shared ++ n64AndDreamcast ++ streamingHost;
  rgds = shared ++ n64AndDreamcast ++ streamingHost;
  r36tmax = shared;
  # The Mini V2 reaches a computer over its USB network gadget; key-only SSH
  # is its copy and debug path.
  rpminiv2 = shared ++ n64AndDreamcast ++ ds ++ streamingHost ++ [ plugins.korri-plugin-ssh ];
  odin2portal = shared ++ n64AndDreamcast ++ ds ++ streamingHost;
  # RG35XXSP is not a product image yet. Its choice becomes active only when
  # measured display/input facts allow the product module and image to land.
  rg35xxsp = shared;
}
