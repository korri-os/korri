#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 dtc
"""Read the real compiled board DTB; never infer radio wiring from source text."""
import argparse
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dtb', type=Path)
    dtb = parser.parse_args().dtb

    def get(node, prop, kind=None):
        args = ['fdtget'] + (['-t', kind] if kind else [])
        return subprocess.check_output(args + [str(dtb), node, prop], text=True).strip()

    children = subprocess.check_output(['fdtget', '-l', str(dtb), '/'], text=True).split()
    sdio = '/' + next(n for n in children if n.endswith('@ff380000'))
    emmc = '/' + next(n for n in children if n.endswith('@ff390000'))
    assert get(sdio, 'max-frequency', 'u') == '25000000'
    assert get(sdio, 'status') == 'okay'
    props = subprocess.check_output(['fdtget', '-p', str(dtb), sdio], text=True).split()
    assert 'supports-rk912' in props
    assert get(sdio + '/wifi@1', 'reg', 'u') == '1'
    assert get(sdio + '/wifi@1', 'interrupt-names') == 'host-wake'
    assert get(sdio + '/wifi@1', 'interrupts', 'u') == '5 4'
    gpio0 = get('/pinctrl/gpio@ff040000', 'phandle', 'u')
    assert get(sdio + '/wifi@1', 'interrupt-parent', 'u') == gpio0
    assert get('/sdio-pwrseq', 'reset-gpios', 'u') == f'{gpio0} 2 1'
    assert get(emmc, 'status') == 'disabled'
    print('Compiled DTB: 25MHz SDIO, RK915 quirk, PA5 level-high host wake, eMMC disabled.')


if __name__ == '__main__':
    main()
