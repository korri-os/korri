#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 dtc
"""Reject malformed amplifier routes using copies of the actual compiled DTB."""
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

    amp = '/audio-amplifier'
    card = '/rk817-sound'
    pin = '/pinctrl/speaker/spk-amp-enable-h'
    gpio3 = cells('/pinctrl/gpio@ff270000', 'phandle')[0]
    pull_none = cells('/pinctrl/pcfg-pull-none', 'phandle')[0]
    pull_up = cells('/pinctrl/pcfg-pull-up', 'phandle')[0]
    route = bytes(int(value, 16) for value in subprocess.check_output(
        ['fdtget', '-t', 'bx', str(dtb), card, 'simple-audio-card,routing'], text=True,
    ).split()).rstrip(b'\0').decode().split('\0')

    def put(node, prop, values, kind='u'):
        return ['-t', kind], [node, prop, *map(str, values)]

    cases = [
        ('missing amplifier', ['-r'], [amp], 'one native'),
        ('disabled amplifier', *put(amp, 'status', ['disabled'], 's'), 'amp disabled'),
        ('active-low enable', *put(amp, 'enable-gpios', [gpio3, 7, 1]), 'PA7 active-high'),
        ('motor pin', *put(amp, 'enable-gpios', [gpio3, 6, 0]), 'PA7 active-high'),
        ('alternate pin function', *put(pin, 'rockchip,pins', [3, 7, 1, pull_none]), 'GPIO mode'),
        ('pull-up', *put(pin, 'rockchip,pins', [3, 7, 0, pull_up]), 'pull-none'),
        ('invented supply', *put(amp, 'VCC-supply', [gpio3]), 'ungrounded amplifier'),
        ('prefix mismatch', *put(amp, 'sound-name-prefix', ['Amp'], 's'), 'prefix'),
        ('wrong aux component', *put(card, 'simple-audio-card,aux-devs', [gpio3]), 'auxiliary'),
        ('card rename', *put(card, 'simple-audio-card,name', ['rk817_ext'], 's'), 'identity'),
        ('switch rename', *put(card, 'simple-audio-card,pin-switches', ['Speaker'], 's'), 'UCM contract'),
        ('disabled card', *put(card, 'status', ['disabled'], 's'), 'card disabled'),
        ('jack polarity', *put(card, 'simple-audio-card,hp-det-gpios', [gpio3, 22, 1]), 'headphone detection'),
        ('clock changed', *put(card, 'simple-audio-card,mclk-fs', [128]), '256x MCLK'),
        ('mic route removed', *put(card, 'simple-audio-card,routing', route[2:], 's'), 'existing mic'),
        ('direct SPKO', *put(card, 'simple-audio-card,routing', [*route[:6], 'Internal Speakers', 'SPKO'], 's'), 'no SPKO'),
    ]
    for offset in (6, 8, 10, 12):
        cases.append((f'missing channel edge {offset // 2}', *put(
            card, 'simple-audio-card,routing', route[:offset] + route[offset + 2:], 's',
        ), 'two-channel'))
    with tempfile.TemporaryDirectory() as directory:
        for name, options, args, expected in cases:
            candidate = Path(directory) / 'board.dtb'
            shutil.copyfile(dtb, candidate)
            subprocess.run(['fdtput', *options, str(candidate), *args], check=True)
            result = subprocess.run(
                [sys.executable, str(checker), str(candidate)], text=True, capture_output=True,
            )
            assert result.returncode != 0 and expected in result.stderr, (name, result.stderr)
            print(f'PASS negative control: {name} refused ({expected})')
    print(f'{len(cases)} compiled speaker DTB mutations refused; no hardware exercised.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dtb', type=Path)
    parser.add_argument('checker', type=Path)
    args = parser.parse_args()
    check_mutations(args.dtb, args.checker)


if __name__ == '__main__':
    main()
