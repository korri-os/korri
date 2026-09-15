#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 dtc
"""Reject unsafe PX30 candidates by mutating a real, compiled DTB."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def check_mutations(dtb: Path, checker: Path) -> None:
    subprocess.run([sys.executable, str(checker), str(dtb)], check=True)

    def cell(node: str, prop: str) -> str:
        return subprocess.check_output(
            ['fdtget', '-t', 'u', str(dtb), node, prop], text=True,
        ).strip()

    def put(node: str, prop: str, values: list[str | int], kind: str = 'u') -> list[str]:
        return ['-t', kind, node, prop, *map(str, values)]

    hantro = '/video-codec@ff442000'
    vepu = '/vepu@ff442000'
    srv = '/mpp-srv'
    mmu = '/iommu@ff442800'
    power = cell('/power-management@ff000000/power-controller', 'phandle')
    cru = cell('/clock-controller@ff2b0000', 'phandle')
    grf = cell('/syscon@ff140000', 'phandle')
    cases = [
        ('hantro enabled', put(hantro, 'status', ['okay'], 's'), 'status'),
        ('encoder enabled', put(vepu, 'status', ['okay'], 's'), 'status'),
        ('service enabled', put(srv, 'status', ['okay'], 's'), 'status'),
        ('generic binding', put(vepu, 'compatible', ['rockchip,vpu-encoder-v2'], 's'), 'compatible'),
        ('exclusive resets', put(vepu, 'reset-names', ['video_a', 'video_h'], 's'), 'reset-names'),
        ('wrong interrupt', put(vepu, 'interrupts', [0, 81, 4]), 'interrupts'),
        ('wrong register base', put(vepu, 'reg', [0, 0xff442400, 0, 0x400]), 'reg'),
        ('wrong register size', put(vepu, 'reg', [0, 0xff442000, 0, 0x800]), 'reg'),
        ('wrong interrupt controller', put(vepu, 'interrupt-parent', [grf]), 'interrupt-parent'),
        ('wrong clock', put(vepu, 'clocks', [cru, 175, cru, 245]), 'clocks'),
        ('wrong clock controller', put(vepu, 'clocks', [grf, 175, grf, 244]), 'clocks'),
        ('wrong reset', put(vepu, 'resets', [cru, 36, cru, 39]), 'resets'),
        ('CPU power domain', put(vepu, 'power-domains', [power, 1]), 'power-domains'),
        ('wrong IOMMU', put(vepu, 'iommus', [grf]), 'iommus'),
        ('wrong service', put(vepu, 'rockchip,srv', [grf]), 'rockchip,srv'),
        ('wrong queue', put(vepu, 'rockchip,taskqueue-node', [1]), 'taskqueue-node'),
        ('wrong reset group', put(vepu, 'rockchip,resetgroup-node', [1]), 'resetgroup-node'),
        ('extra queue', put(srv, 'rockchip,taskqueue-count', [2]), 'taskqueue-count'),
        ('extra reset group', put(srv, 'rockchip,resetgroup-count', [2]), 'resetgroup-count'),
        ('wrong GRF', put(srv, 'rockchip,grf', [cru]), 'rockchip,grf'),
        ('wrong GRF offset', put(srv, 'rockchip,grf-offset', [0x414]), 'grf-offset'),
        ('wrong GRF selection', put(srv, 'rockchip,grf-values', [0x80008000] * 3), 'grf-values'),
        ('missing GRF', ['-d', srv, 'rockchip,grf'], 'FDT_ERR_NOTFOUND'),
        ('wrong IOMMU power domain', put(mmu, 'power-domains', [power, 1]), 'power-domains'),
        ('wrong IOMMU interrupt', put(mmu, 'interrupts', [0, 80, 4]), 'interrupts'),
    ]

    with tempfile.TemporaryDirectory() as temp:
        for label, operation, expected in cases:
            candidate = Path(temp) / 'mutated.dtb'
            shutil.copyfile(dtb, candidate)
            # fdtput options precede the DTB filename.
            split = 2 if operation[0] == '-t' else 1
            subprocess.run(
                ['fdtput', *operation[:split], str(candidate), *operation[split:]],
                check=True,
            )
            result = subprocess.run(
                [sys.executable, str(checker), str(candidate)],
                capture_output=True, text=True,
            )
            assert result.returncode != 0, f'{label}: checker accepted the mutation'
            assert expected in result.stderr, f'{label}: unexpected rejection: {result.stderr}'
    print(f'PX30 offline candidate: {len(cases)} rejected mutations')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dtb', type=Path)
    parser.add_argument('checker', type=Path)
    args = parser.parse_args()
    check_mutations(args.dtb, args.checker)


if __name__ == '__main__':
    main()
