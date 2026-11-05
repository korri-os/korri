# The catalog archive proofs are checked against actual locally registered
# bytes before restore-all. The command has no acquisition or native effects.
#
# The proofs it registers are signatures in the device's Nix database, so they
# persist. It runs once for each shipped metadata set (the first boot of a new
# image or system), not on every boot: on the RG353M it took 71 s and held up
# korrid on every boot (2026-11-05). A skipped run satisfies requiredBy.
{ metadata }:
let
  stamp = "/var/lib/korri-plugin-offline-proofs/${baseNameOf (toString metadata)}";
in
{
  description = "Register signed catalog archive proofs for shipped plugins";
  requiredBy = [ "korri-plugin-host.service" ];
  before = [ "korri-plugin-host.service" ];
  after = [ "systemd-tmpfiles-setup.service" ];
  unitConfig.ConditionPathExists = "!${stamp}";
  serviceConfig = {
    Type = "oneshot";
    RemainAfterExit = true;
    UMask = "0077";
    StateDirectory = "korri-plugin-offline-proofs";
  };
  script = ''
    set -euo pipefail
    ${metadata}/verify
    touch ${stamp}
  '';
}
