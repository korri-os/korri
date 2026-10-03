# Package choices recorded in
# .scratch/consistent-korri-product/evidence/rocknix-recommendations.md.
# Simon removed PSP, DraStic, YabaSanshiro, Dolphin, Azahar and AetherSX2
# from this release's defaults. These are build-time choices, not a persisted
# plugin schema or proof that a device can run each selected game.
{ plugins }:
let
  select = names: map (name: plugins."korri-plugin-${name}") names;
  # Owner decision: Starter pack ships on every product device. Image seeding
  # creates one receipt per selected package, so its exact FAKE-08 dependency
  # must be selected too, not only retained in the cartridge pack's closure.
  shared = select [
    "fake08"
    "starter-pack"
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
  n64AndDreamcast = select [
    "mupen64plus"
    "flycast"
  ];
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
  # The two H700 boards use ROCKNIX's H700 table (evidence/rocknix-
  # recommendations.md). No streaming host until an encoder result exists.
  rg35xxsp = shared ++ ssh;
  rg35xxpro = shared ++ ssh;
}
