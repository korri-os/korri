# The catalog archive proofs are checked against actual locally registered
# bytes before restore-all. The command has no acquisition or native effects.
{ metadata }:
{
  description = "Register signed catalog archive proofs for shipped plugins";
  requiredBy = [ "korri-plugin-host.service" ];
  before = [ "korri-plugin-host.service" ];
  after = [ "systemd-tmpfiles-setup.service" ];
  serviceConfig = {
    Type = "oneshot";
    RemainAfterExit = true;
    UMask = "0077";
  };
  script = ''
    set -euo pipefail
    ${metadata}/verify
  '';
}
