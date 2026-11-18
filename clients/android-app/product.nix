# Android consumes the same relay policy as the Linux product. This generated
# build input is public configuration, not a second hand-maintained relay list.
{ pkgs, korri }:
let
  product = import ../../nix/product/requirements.nix { inherit korri; };
in
pkgs.writeText "korri-android-relays.json" (builtins.toJSON product.constants.relays)
