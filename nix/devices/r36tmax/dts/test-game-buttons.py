#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 dtc
"""Check rejection of malformed button bindings using copies of a compiled DTB."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def check_mutations(dtb, checker):
    subprocess.run([sys.executable, str(checker), str(dtb)], check=True)

    def cells(node, prop):
        return subprocess.check_output(
            ['fdtget', '-t', 'u', str(dtb), node, prop], text=True,
        ).split()

    def put(node, prop, values):
        return ['-t', 'u'], [node, prop, *map(str, values)]

    gpio2 = cells('/pinctrl/gpio@ff260000', 'phandle')[0]
    gpio3 = cells('/pinctrl/gpio@ff270000', 'phandle')[0]
    pin_node = '/pinctrl/btns/btn-pins'
    pins = cells(pin_node, 'rockchip,pins')
    button = '/gpio-keys-polled/button-up'
    bad_pull = pins.copy()
    bad_pull[3] = cells('/pinctrl/pcfg-pull-none', 'phandle')[0]
    bad_function = pins.copy()
    bad_function[2] = '1'
    bad_pinctrl = pins.copy()
    bad_pinctrl[0:2] = ['3', '6']
    cases = [
        ('missing Fn', ['-r'], ['/gpio-keys-polled/button-fn'], 'exactly 17'),
        ('wrong code', *put(button, 'linux,code', [0x221]), 'mapping mismatch'),
        ('active high', *put(button, 'gpios', [gpio3, 22, 0]), 'active-low'),
        ('PD2 volume', *put(button, 'gpios', [gpio3, 26, 1]), 'excluded'),
        ('PD3 volume', *put(button, 'gpios', [gpio3, 27, 1]), 'excluded'),
        ('PA6 motor', *put(button, 'gpios', [gpio3, 6, 1]), 'excluded'),
        ('GPIO3 PB7 reset', *put(button, 'gpios', [gpio3, 15, 1]), 'excluded'),
        ('GPIO2 PB7 mux', *put(button, 'gpios', [gpio2, 15, 1]), 'excluded'),
        ('pull none', *put(pin_node, 'rockchip,pins', bad_pull), 'plain pull-up'),
        ('alternate function', *put(pin_node, 'rockchip,pins', bad_function), 'GPIO mode'),
        ('excluded pinctrl', *put(pin_node, 'rockchip,pins', bad_pinctrl), 'excluded pin'),
        ('wrong polling', *put('/gpio-keys-polled', 'poll-interval', [30]), '10 ms'),
        ('joystick identity', ['-t', 's'],
         ['/gpio-keys-polled', 'label', 'r36s_Gamepad'], 'native identity'),
        ('18th button', ['-p', '-t', 'u'],
         ['/gpio-keys-polled/button-extra', 'linux,code', '1'], 'exactly 17'),
    ]
    with tempfile.TemporaryDirectory() as directory:
        for name, options, args, expected in cases:
            candidate = Path(directory) / 'board.dtb'
            shutil.copyfile(dtb, candidate)
            subprocess.run(['fdtput', *options, str(candidate), *args], check=True)
            result = subprocess.run(
                [sys.executable, str(checker), str(candidate)],
                text=True, capture_output=True,
            )
            assert result.returncode != 0 and expected in result.stderr, (name, result.stderr)
            print(f'PASS negative control: {name} refused ({expected})')
    print(f'{len(cases)} compiled-DTB mutations refused; no hardware exercised.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dtb', type=Path)
    parser.add_argument('checker', type=Path)
    args = parser.parse_args()
    check_mutations(args.dtb, args.checker)


if __name__ == '__main__':
    main()
