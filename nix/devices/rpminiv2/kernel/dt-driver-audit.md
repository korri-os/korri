# Compiled DT driver coverage

`dt-driver-check.nix` checks the **product** DTB against outputs of the same
kernel derivation. Both `../module-check.nix` and direct product SD-image
assembly require this check. Recovery is unchanged. Run the board gate with
`nix run .#rpminiv2-check`.

The output is a TSV report. A `MISSING` row names the full node path and every
compatible string and fails the build. Input errors also fail. Each compatible
node is reported as `DRIVER`, `DATA`, `REVIEWED`, `DISABLED`, or `MISSING`.
A node is enabled only when it and every ancestor have absent, `okay`, or `ok`
status. A child without `compatible` is not an independent driver claim.

This is a compilation/packaging gate, **not proof of runtime binding or working
hardware**. It does not check firmware, module loading, dependency resolution,
probe errors, audio routing, or userspace. A matching driver may implement only
one function of a multifunction device. It does not claim that every property
or child description is valid. Runtime acceptance remains necessary.

## Evidence, not a second Kconfig map

| Evidence | Producer and use |
| --- | --- |
| `modules.builtin.modinfo` | Kbuild's NUL-delimited builtin aliases. Some builtin-only objects are absent from `modules.builtin`, so that file is not an authoritative filter. |
| `modules.alias` plus actual `.ko`, `.ko.xz`, `.ko.gz`, or `.ko.zst` | depmod's aliases, accepted only when the owning module is packaged. A stale alias without its file cannot pass. |
| ELF `__of_table_*` records | `include/linux/of.h:_OF_DECLARE`. Read the compiled compatible and check the callback points to executable code. Linux 7.2 `RESERVEDMEM_OF_DECLARE` points to five-function `reserved_mem_ops`; check those callbacks instead. This covers fixed clocks, early IRQ controllers and reserved DMA memory without module aliases. |
| ELF `qcom_pcie_match` | `drivers/pci/controller/dwc/pcie-qcom.c:qcom_pcie_driver` uses this table with `builtin_platform_driver`, without exporting OF module aliases. |
| ELF `fastrpc_match_table` | `drivers/misc/fastrpc.c:fastrpc_cb_driver` uses this table for the context-bank **drivers**. They are not exempt child nodes. The rpmsg table alone does not cover them. |
| ELF `psci_of_match` | `drivers/firmware/psci/psci.c:psci_dt_init` and `drivers/cpuidle/cpuidle-psci-domain.c:psci_cpuidle_domain_driver` consume these tables without module aliases. |
| ELF `syscon_node_to_regmap` | `drivers/mfd/syscon.c` creates regmaps on demand, not through an OF driver table. Require the compiled function only for the exact SM8250 TCSR path/compatibles. |

The three non-exported table names are source-grounded exceptions to **alias
lookup**, not permission to omit drivers. Their compatible lists come from the
ELF, not a copied mapping. Removing their compiled records fails coverage.
These consumers are builtin in the current ROCKNIX configuration. If they move
to modules without exporting aliases, coverage fails closed and needs review.

OF aliases follow `drivers/of/module.c:of_modalias`, including the node name,
device type, and all fallback compatible strings. SoundWire devices use the
native ID format from `drivers/soundwire/slave.c:sdw_of_find_slaves` and
`drivers/soundwire/bus_type.c:sdw_slave_modalias`.

## Descriptions that have no independent driver

These are explicitly reported as `DATA`, not silently treated as driver matches:

| Description | Narrow condition and Linux 7.2 consumer |
| --- | --- |
| Board identity | Exact root path and RP Mini V2 root compatible list from the board DTS. |
| OPP tables | Exact `operating-points-v2` compatible, with a phandle referenced by an enabled node's `operating-points-v2` property. `drivers/opp/of.c:_opp_of_get_opp_desc_node` consumes it. Unreferenced tables fail. |
| CPU cache topology | Exact nine cache paths under the eight existing CPUs, with only `cache` compatible. `drivers/base/cacheinfo.c:cache_setup_of_node`. |
| CPU idle states | Exact three existing idle-state paths and compatible lists. `drivers/cpuidle/dt_idle_states.c` and `drivers/pmdomain/core.c:genpd_iterate_idle_states`. |
| USB-C connector | Exact connector child of the PM8150B Type-C node, with the expected parent compatible. `drivers/usb/typec/tcpm/qcom/qcom_pmic_typec.c:device_get_named_child_node(..., "connector")`. The parent still requires a driver. |

No wildcard excludes Qualcomm nodes, PMIC children, FastRPC context banks, or
unknown bus children. The source for these rules was read from the same pinned
Linux 7.2 archive as `default.nix`; board paths come from the supplied DTS and
compiled DTB. The old runtime audit is an observation, not the coverage source.

## Reviewed hardware omissions

The fixed owner-approved list is CPU cores/timers/CPU PMU, CoreSight, CAMCC,
simple-framebuffer, simple-battery, and GMU. The UFS host is compiled for the
internal root and is covered as a driver. `REVIEWED` requires **both the exact existing path and compatible list**.
It never exempts descendants. Existing compiled coverage takes precedence and
is reported as `DRIVER`. A new path or changed compatible list needs review.
CPU PMU does not exempt the two Qualcomm bandwidth monitors. There are no additional hardware omissions.

## Tests and direct use

Local executables use Nix shebangs:

```
./nix/devices/rpminiv2/kernel/dt-driver-audit.test.py -v
./nix/devices/rpminiv2/kernel/dt-driver-audit.py \
  --dtb "$kernel/dtbs/qcom/sm8250-retroidpocket-rpminiv2.dtb" \
  --modules "$modules/lib/modules/7.2.0" \
  --vmlinux "$dev/vmlinux"
```

The three direct inputs must belong to the same kernel build. Nix integration
sets them from one derivation, rather than guessing store paths. The standalone
command trusts the caller to maintain this association and depmod metadata.
It does not compile, load, install, or contact a device.

The tests compile small DTB and ELF fixtures. They cover missing touch, LED,
RTC and PMIC temperature drivers; builtin and packaged module aliases; stale
aliases; fallback compatibles; SoundWire IDs; compiled early and unexported
OF tables; disabled ancestors; narrow data classifications and exceptions.

Set `RP_MINIV2_AUDIT_DTB`, `RP_MINIV2_AUDIT_MODULES`, and
`RP_MINIV2_AUDIT_VMLINUX` to also run real-kernel mutation tests. Those require
initial driver coverage, then remove each driver's evidence and demand
`MISSING`. They also check the real disabled USB parent. The Nix gate always
runs these tests. Standalone tests without those inputs explicitly skip them.

### Host-only validation on 2026-09-26

| Input/test | Verified result |
| --- | --- |
| Existing product kernel `aw910rsrn01jhmka37gklxacrkfn0m55`, modules `r6cpbybb19amq27212brmxn6ql20v84p`, dev `1j8fbl5klgzbb4059lrqdmf648yvz61i` | Exit 1: 13 missing nodes: 8 multicolor LED groups, touchscreen, SDAM, 3 PMIC temperature alarms. 168 driver-covered, 29 data, 39 reviewed, 61 disabled. RTC already has compiled coverage; runtime unbound does not mean uncompiled. |
| Same kernel plus a **temporary host fixture** containing the four already-built modules from `2174h2bf3ns8njqfgzffcsn4i9dbcv9q-rpminiv2-test-modules-aarch64-unknown-linux-gnu-7.2.0`, with depmod regenerated | Exit 0: 181 driver-covered, 29 data, 39 reviewed, 61 disabled. All 15 tests passed, including real-kernel mutations. No device was modified. This is not the new full ROCKNIX product build. |
| Standalone fixtures | 13 tests passed; 2 real-kernel tests explicitly skipped. |
| Nix sandbox | The standalone check derivation passed against the merged host fixture, including all 15 tests. The real board-check dependency failed against the unmodified product outputs with the same 13 missing nodes. `nix eval --raw .#checks.x86_64-linux.rpminiv2.drvPath` passed. |

The new full ROCKNIX product kernel also passed this audit and all 15 tests
on Zao. The input paths were `i6xmnqi4w9rpkir25qvjnli0agdg59dn`, modules
`05nccfxqi9x5zmzkayfhq7x1gncbs2m8`, and dev
`bvw247rlsyj0hicc33y4nd4a0j3y1j32`. The later parser-only rebuild,
`v7y2488amzg4rbzi8gs96nv5213ml76p`, passed again through the board gate.
Both report 182 driver-covered, 29 data, 38 reviewed, 61 disabled and zero
missing nodes. `../default.nix` makes the product image depend on this same
derivation before root population. Recovery has no audit dependency.
