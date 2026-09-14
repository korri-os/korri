{
  runCommand,
  kmod,
  binutils,
  python3,
  patch,
  xz,
  driver,
  firmware,
  kernel,
  source,
}:
runCommand "rk915-package-checks"
  {
    nativeBuildInputs = [
      kmod
      binutils
      python3
      patch
      xz
    ];
    # This check consumes locally supplied, non-redistributable firmware.
    preferLocalBuild = true;
    allowSubstitutes = false;
  }
  ''
    set -euo pipefail
    expect_failure() {
      if "$@" > failure.log 2>&1; then
        echo "unexpected success: $*" >&2
        exit 1
      fi
    }

    bash ${./check-module.sh} ${driver} ${kernel.dev} ${kernel.modDirVersion}
    expect_failure bash ${./check-module.sh} /missing ${kernel.dev} ${kernel.modDirVersion}
    expect_failure bash ${./check-module.sh} ${driver} ${kernel.dev} wrong-release

    # Use the real module with one deliberately corrupted kernel symbol CRC.
    mkdir -p mismatched/lib/modules/${kernel.modDirVersion}/build
    python3 - ${kernel.dev}/lib/modules/${kernel.modDirVersion}/build/Module.symvers \
      mismatched/lib/modules/${kernel.modDirVersion}/build/Module.symvers <<'PY'
    import pathlib
    import sys

    lines = pathlib.Path(sys.argv[1]).read_text().splitlines()
    changed = False
    for i, line in enumerate(lines):
        fields = line.split()
        if fields[1] == "module_layout":
            fields[0] = hex(int(fields[0], 16) ^ 1)
            lines[i] = "\t".join(fields)
            changed = True
    assert changed
    pathlib.Path(sys.argv[2]).write_text("\n".join(lines) + "\n")
    PY
    expect_failure bash ${./check-module.sh} ${driver} "$PWD/mismatched" ${kernel.modDirVersion}
    grep -F 'kernel CRC mismatch: module_layout' failure.log

    bash ${./check-firmware.sh} ${firmware}
    cmp ${source}/firmware/rk915_fw.bin ${firmware}/lib/firmware/rk915_fw.bin
    cmp ${source}/firmware/rk915_patch.bin ${firmware}/lib/firmware/rk915_patch.bin
    cp -r ${firmware} damaged
    chmod -R u+w damaged
    printf '\000' >> damaged/lib/firmware/rk915_fw.bin
    expect_failure bash ${./check-firmware.sh} "$PWD/damaged"
    grep -F 'rk915_fw.bin: FAILED' failure.log
    rm damaged/lib/firmware/rk915_fw.bin
    expect_failure bash ${./check-firmware.sh} "$PWD/damaged"
    grep -F 'rk915_fw.bin: FAILED open or read' failure.log

    # Validate applicability of the local patch against the exact source.
    # The separate mmc-check runs the patched parser with sanitizers.
    mkdir linux
    tar -xJf ${kernel.src} -C linux --strip-components=1 \
      linux-${kernel.version}/drivers/mmc/core/host.c \
      linux-${kernel.version}/drivers/mmc/core/sdio.c \
      linux-${kernel.version}/drivers/mmc/core/sdio_cis.c \
      linux-${kernel.version}/drivers/mmc/host/dw_mmc.c \
      linux-${kernel.version}/include/linux/mmc/host.h
    patch --dry-run --fuzz=0 -d linux -p1 < ${./mmc-support.patch}
    touch "$out"
  ''
