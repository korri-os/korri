#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 dtc
"""Check the disabled PX30 MPP candidate, not encoder operation."""
import argparse
from pathlib import Path
import subprocess


# rockchip-linux/kernel@470f9dccbdc42e7b8a824d0a5c5640a10e9457d2:
# arch/arm64/boot/dts/rockchip/px30.dtsi:1627-1694;
# include/dt-bindings/{clock/px30-cru.h,power/px30-power.h}.
def check(dtb: Path) -> None:
    def get(node: str, prop: str, kind: str = 's') -> str:
        return subprocess.check_output(
            ['fdtget', '-t', kind, str(dtb), node, prop], text=True,
        ).strip()

    def cells(node: str, prop: str) -> list[int]:
        return [int(value) for value in get(node, prop, 'u').split()]

    def expect(node: str, prop: str, expected: list[int] | str) -> None:
        actual = get(node, prop) if isinstance(expected, str) else cells(node, prop)
        assert actual == expected, f'{node} {prop}: expected {expected}, got {actual}'

    # Resolve by actual phandles. Their numeric allocation changes with dtc.
    def phandle(node: str) -> int:
        value = cells(node, 'phandle')
        assert len(value) == 1, f'{node}: expected one phandle'
        return value[0]

    hantro = '/video-codec@ff442000'
    vepu = '/vepu@ff442000'
    srv = '/mpp-srv'
    mmu = '/iommu@ff442800'
    cru = phandle('/clock-controller@ff2b0000')
    power = phandle('/power-management@ff000000/power-controller')
    gic = phandle('/interrupt-controller@ff131000')
    grf = phandle('/syscon@ff140000')

    expect(hantro, 'status', 'disabled')
    expect(srv, 'compatible', 'rockchip,mpp-service')
    expect(srv, 'status', 'disabled')
    expect(srv, 'rockchip,taskqueue-count', [1])
    expect(srv, 'rockchip,resetgroup-count', [1])
    expect(srv, 'rockchip,grf', [grf])
    expect(srv, 'rockchip,grf-offset', [0x410])
    expect(srv, 'rockchip,grf-values', [0x80008000, 0x80000000, 0x80000000])
    expect(srv, 'rockchip,grf-names', 'grf_rkvdec grf_vdpu2 grf_vepu2')

    expect(vepu, 'compatible', 'rockchip,vpu-encoder-px30')
    expect(vepu, 'status', 'disabled')
    expect(vepu, 'reg', [0, 0xff442000, 0, 0x400])
    expect(vepu, 'interrupt-parent', [gic])
    expect(vepu, 'interrupts', [0, 80, 4])
    expect(vepu, 'interrupt-names', 'irq_enc')
    expect(vepu, 'clocks', [cru, 175, cru, 244])
    expect(vepu, 'clock-names', 'aclk_vcodec hclk_vcodec')
    expect(vepu, 'resets', [cru, 36, cru, 38])
    expect(vepu, 'reset-names', 'shared_video_a shared_video_h')
    expect(vepu, 'power-domains', [power, 11])
    expect(vepu, 'iommus', [phandle(mmu)])
    expect(vepu, 'rockchip,srv', [phandle(srv)])
    expect(vepu, 'rockchip,taskqueue-node', [0])
    expect(vepu, 'rockchip,resetgroup-node', [0])

    expect(mmu, 'compatible', 'rockchip,iommu')
    expect(mmu, 'reg', [0, 0xff442800, 0, 0x100])
    expect(mmu, 'interrupts', [0, 81, 4])
    expect(mmu, '#iommu-cells', [0])
    expect(mmu, 'clocks', [cru, 175, cru, 244])
    expect(mmu, 'clock-names', 'aclk iface')
    expect(mmu, 'power-domains', [power, 11])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dtb', type=Path)
    args = parser.parse_args()
    check(args.dtb)


if __name__ == '__main__':
    main()
