# Off-device build-time gate. All three inputs come from one kernel derivation;
# never use the source .config or a running device as a coverage proxy.
{ pkgs, kernel }:
let
  python = pkgs.python3.withPackages (p: [ p.libfdt p.pyelftools ]);
in
pkgs.runCommand "rpminiv2-dt-driver-check"
  {
    nativeBuildInputs = [ python pkgs.dtc pkgs.stdenv.cc ];
    RP_MINIV2_AUDIT_DTB = "${kernel}/dtbs/qcom/sm8250-retroidpocket-rpminiv2.dtb";
    RP_MINIV2_AUDIT_MODULES = "${kernel.modules}/lib/modules/${kernel.modDirVersion}";
    RP_MINIV2_AUDIT_VMLINUX = "${kernel.dev}/vmlinux";
  }
  ''
    cp ${./dt-driver-audit.py} dt-driver-audit.py
    cp ${./dt-driver-audit.test.py} dt-driver-audit.test.py
    python3 dt-driver-audit.py \
      --dtb "$RP_MINIV2_AUDIT_DTB" \
      --modules "$RP_MINIV2_AUDIT_MODULES" \
      --vmlinux "$RP_MINIV2_AUDIT_VMLINUX" > report.tsv || {
        cat report.tsv
        # Keep concrete failures at the end, visible in Nix's log tail.
        echo "Missing DT driver coverage:" >&2
        grep '^MISSING' report.tsv >&2 || true
        exit 1
      }
    python3 dt-driver-audit.test.py -v
    cp report.tsv "$out"
  ''
