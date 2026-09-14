#!/usr/bin/env python3
"""Check real binman outputs; offsets come from U-Boot's rksd/PX30 contract."""

import hashlib
from pathlib import Path
import sys

import libfdt
from elftools.elf.elffile import ELFFile


def check(build: Path, ddr_path: Path, bl31_path: Path) -> None:
    config = dict(
        line.split("=", 1)
        for line in (build / ".config").read_text().splitlines()
        if line.startswith("CONFIG_") and "=" in line
    )
    for setting in (
        "CONFIG_ROCKCHIP_PX30",
        "CONFIG_ROCKCHIP_EXTERNAL_TPL",
        "CONFIG_SPL",
        "CONFIG_SPL_ATF",
        "CONFIG_SPL_LOAD_FIT",
        "CONFIG_DOS_PARTITION",
        "CONFIG_FS_EXT4",
        "CONFIG_BOOTMETH_EXTLINUX",
    ):
        assert config.get(setting) == "y", f"Missing requirement: {setting}"
    assert "CONFIG_TPL" not in config, "Open TPL must not be built"
    assert "CONFIG_SPL_BOOTROM_SUPPORT" not in config, (
        "SPL must load FIT, not return to ROM"
    )
    assert "CONFIG_SPL_ROCKCHIP_BACK_TO_BROM" not in config
    assert not (build / "tpl/u-boot-tpl.bin").exists(), "Unexpected open TPL"

    ddr = ddr_path.read_bytes()
    # Verified against rkbin f43a462e's RKBOOT/RK3326MINIALL.ini input.
    assert hashlib.sha256(ddr).hexdigest() == (
        "2df577824953bea3584282e7f5118c96620e1348dbc378d50604fe07c70eb3e4"
    ), "DDR input differs from the reviewed rkbin firmware"
    assert len(ddr) == 10100
    idb = (build / "idbloader.img").read_bytes()
    spl = (build / "spl/u-boot-spl.bin").read_bytes()
    # tools/rkcommon.h: RK_INIT_OFFSET=4 sectors, RK_SIZE_ALIGN=2048.
    # tools/rkcommon.c: PX30 uses RK33 and does not RC4-encode payloads.
    assert idb[2048 : 2048 + len(ddr)] == ddr, "DDR bytes absent from idbloader"
    spl_offset = 2048 + ((len(ddr) + 2047) // 2048) * 2048
    assert idb[spl_offset : spl_offset + len(spl)] == spl, "SPL bytes differ"
    assert ((len(ddr) + 2047) // 2048) * 2048 <= 0x2800, "DDR exceeds PX30 limit"

    fit = (build / "u-boot.itb").read_bytes()
    fdt = libfdt.Fdt(fit)
    images = fdt.path_offset("/images")
    with bl31_path.open("rb") as handle:
        elf = ELFFile(handle)
        atf_segments = [
            segment
            for segment in elf.iter_segments()
            if segment["p_type"] == "PT_LOAD" and segment["p_memsz"]
        ]
        expected = {
            f"atf-{index}": (segment["p_paddr"], segment.data())
            for index, segment in enumerate(atf_segments, 1)
        }
    names = []
    image = fdt.first_subnode(images)
    while image >= 0:
        name = fdt.get_name(image)
        names.append(name)
        # Compare external FIT payloads with the actual build inputs. This
        # board does not enable FIT signatures or per-image hash nodes.
        position = fdt.getprop(image, "data-position", quiet=(libfdt.NOTFOUND,))
        if isinstance(position, int):
            position = fdt.totalsize() + fdt.getprop(image, "data-offset").as_uint32()
        else:
            position = position.as_uint32()
        size = fdt.getprop(image, "data-size").as_uint32()
        data = fit[position : position + size]
        assert len(data) == size and size > 0, f"Truncated FIT image: {name}"
        if name in expected:
            address, original = expected[name]
            assert fdt.getprop(image, "load").as_uint32() == address
        elif name == "u-boot":
            original = (build / "u-boot-nodtb.bin").read_bytes()
        elif name == "fdt-1":
            original = (build / "u-boot.dtb").read_bytes()
        else:
            raise AssertionError(f"Unexpected FIT image: {name}")
        assert data == original, f"FIT payload differs from build input: {name}"
        image = fdt.next_subnode(image, quiet=(libfdt.NOTFOUND,))
    assert set(names) == {"u-boot", "fdt-1", *expected}
    configurations = fdt.path_offset("/configurations")
    default = fdt.getprop(configurations, "default").as_str()
    selected = fdt.subnode_offset(configurations, default)
    assert fdt.getprop(selected, "firmware").as_str() == "atf-1"
    assert fdt.getprop(selected, "fdt").as_str() == "fdt-1"

    combined = (build / "u-boot-rockchip.bin").read_bytes()
    # arch/arm/dts/rockchip-u-boot.dtsi places FIT at CONFIG_SPL_PAD_TO.
    fit_offset = int(config["CONFIG_SPL_PAD_TO"], 0)
    assert len(idb) <= fit_offset, "idbloader overlaps FIT"
    assert combined[: len(idb)] == idb, "Combined image has a different loader"
    assert combined[fit_offset:] == fit, "Combined image has a different FIT"
    print(f"PASS: vendor DDR + SPL; FIT payloads ({', '.join(names)}); combined image")


if __name__ == "__main__":
    check(Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]))
