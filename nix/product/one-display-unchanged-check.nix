# Freeze the pre-ADR-0039 Sway generator as a regression oracle, not runtime
# configuration. Single-display devices must keep its complete byte sequence.
{ pkgs, korri }:
let
  inherit (pkgs) lib;
  devices = [
    "odin2portal"
    "rg353m"
    "rpminiv2"
    "r36tmax"
    "rg35xxsp"
    "rg35xxpro"
  ];
  unchanged =
    name:
    let
      c = korri.nixosConfigurations.${name}.config;
      cfg = c.services.korriLinuxHost.compositor;
      d = builtins.head cfg.displays;
      before = ''
        default_border none
        default_floating_border none
        hide_edge_borders both
        xwayland force
        seat * hide_cursor 1500
        ${lib.optionalString (!cfg.localInput.enable) ''
          input "*" events disabled
          ${lib.concatMapStringsSep "\n" (id: ''input "${id}" events enabled'') cfg.allowedInputIdentifiers}
        ''}
        output ${d.name} mode ${d.mode}
        output ${d.name} bg #000000 solid_color
        workspace "korri:game:active" output ${d.name}
        workspace "korri:game:active"
        ${cfg.extraConfig}
      '';
      after = c.systemd.services.korri-compositor.environment.KORRI_SWAY_CONFIG.text;
    in
    assert builtins.length cfg.displays == 1;
    assert d.name == "DSI-1" && cfg.mainDisplay == d.name;
    assert d.x == 0 && d.y == 0 && d.touch == null;
    assert !(lib.hasInfix "WL_OUTPUT" c.services.udev.extraRules);
    assert before == after;
    true;
in
assert lib.all unchanged devices;
pkgs.runCommand "one-display-unchanged" { } ''touch "$out"''
