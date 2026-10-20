# Package choices follow the curated ROCKNIX recommendations.
# Simon removed PSP, DraStic, YabaSanshiro, Dolphin, Azahar and AetherSX2
# from this release's defaults. These are build-time choices, not a persisted
# plugin schema or proof that a device can run each selected game.
{ plugins }:
let
  select = names: map (name: plugins."korri-plugin-${name}") names;
  # Owner decision: Starter pack ships Disabled on every product device; setup
  # offers to enable it offline. Its three runners stay Enabled. Image seeding
  # creates one receipt per selected package, so its exact FAKE-08, Solarus
  # and ZQuest Classic dependencies must be selected too, not only retained in
  # the cartridge pack's closure.
  shared = select [
    "fake08"
    "solarus"
    "zquest-classic"
    "starter-pack"
    # Owner decision 2026-10-18: Gamescope ships on every product device.
    # Installed, it wraps nothing until its device modifier is enabled
    # (ADR 0009).
    "gamescope"
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
  # Steam runs Windows games through ARM64 Proton-CachyOS with FEX for x86
  # helpers. Only the RP Mini V2 has been accepted with it (2026-10-19).
  # The exact required FEX and Proton plugins are selected with it, because
  # image seeding creates a receipt only for selected packages.
  steam = select [
    "steam"
    "fex"
    "proton"
  ];
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
  rpminiv2 = shared ++ n64AndDreamcast ++ ds ++ streamingHost ++ ssh ++ steam;
  odin2portal = shared ++ n64AndDreamcast ++ ds ++ streamingHost ++ ssh;
  # The two H700 boards use ROCKNIX's H700 table.
  # No streaming host until an encoder result exists.
  rg35xxsp = shared ++ ssh;
  rg35xxpro = shared ++ ssh;
}
