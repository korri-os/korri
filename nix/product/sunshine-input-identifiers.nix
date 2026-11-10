# Sunshine v2026.914.233613: src/platform/virtualhid_input.cpp creates these
# profiles. libvirtualhid's Linux backend adds a second, absolute mouse.
# No trackpad is created. Gamepads belong to Korri seats, never to sway.
# sunshine-native-check.nix checks this set against the shipped source hash.
[
  "4617:3:libvirtualhid_Mouse"
  "4617:3:libvirtualhid_Mouse_(Absolute)"
  "4617:2:libvirtualhid_Keyboard"
  "4617:4:libvirtualhid_Touchscreen"
  "4617:6:libvirtualhid_Pen_Tablet"
]
