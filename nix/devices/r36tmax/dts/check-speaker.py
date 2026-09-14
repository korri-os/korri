#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 dtc
"""Check the compiled speaker binding; does not exercise drivers or hardware."""
import argparse
from pathlib import Path
import subprocess


# ROCKNIX/distribution@697d112a64e442e79e3d27b064351d917657301a,
# rk3326-gameconsole-eeclone.dts:292-330,896-900; Linux 6.12.63
# simple-audio-amplifier.yaml and simple-card.yaml define these properties.
ROUTES = [
    'MICL', 'Mic Jack',
    'Headphones', 'HPOL',
    'Headphones', 'HPOR',
    'Internal Speakers', 'Speaker Amp OUTL',
    'Internal Speakers', 'Speaker Amp OUTR',
    'Speaker Amp INL', 'HPOL',
    'Speaker Amp INR', 'HPOR',
]


def check(dtb):
    def get(node, prop, kind='s'):
        return subprocess.check_output(
            ['fdtget', '-t', kind, str(dtb), node, prop], text=True,
        ).strip()

    def cells(node, prop):
        return [int(value) for value in get(node, prop, 'u').split()]

    def strings(node, prop):
        # fdtget -t s flattens spaces inside strings. Read bytes to keep
        # endpoint boundaries ("Internal Speakers" is one string).
        return bytes(int(value, 16) for value in get(node, prop, 'bx').split()).rstrip(
            b'\0',
        ).decode().split('\0')

    nodes, props, phandles = [], {}, {}
    pending = ['/']
    while pending:
        node = pending.pop()
        nodes.append(node)
        props[node] = set(subprocess.check_output(
            ['fdtget', '-p', str(dtb), node], text=True,
        ).split())
        if 'phandle' in props[node]:
            phandles[cells(node, 'phandle')[0]] = node
        pending.extend(node.rstrip('/') + '/' + child for child in subprocess.check_output(
            ['fdtget', '-l', str(dtb), node], text=True,
        ).split())

    def compatible(value):
        return [node for node in nodes if 'compatible' in props[node]
                and value in strings(node, 'compatible')]

    amps = compatible('simple-audio-amplifier')
    assert len(amps) == 1, 'expected one native simple-audio-amplifier'
    amp = amps[0]
    assert props[amp] <= {
        'compatible', 'enable-gpios', 'sound-name-prefix', 'pinctrl-names',
        'pinctrl-0', 'phandle', 'status',
    }, 'no ungrounded amplifier supplies or properties'
    assert 'status' not in props[amp] or get(amp, 'status') == 'okay', 'amp disabled'
    assert get(amp, 'sound-name-prefix') == 'Speaker Amp', 'amp prefix must match routes'
    gpio3 = '/pinctrl/gpio@ff270000'
    assert get(gpio3, 'compatible') == 'rockchip,gpio-bank'
    assert 'gpio-controller' in props[gpio3] and cells(gpio3, '#gpio-cells') == [2]
    assert cells(amp, 'enable-gpios') == [cells(gpio3, 'phandle')[0], 7, 0], \
        'amp enable must be GPIO3 PA7 active-high'
    assert get(amp, 'pinctrl-names') == 'default'
    refs = cells(amp, 'pinctrl-0')
    assert len(refs) == 1, 'one amplifier pinctrl group required'
    pins = cells(phandles[refs[0]], 'rockchip,pins')
    assert len(pins) == 4 and pins[:3] == [3, 7, 0], 'PA7 must use GPIO mode'
    assert props[phandles[pins[3]]] == {'bias-disable', 'phandle'}, 'plain pull-none required'

    cards = compatible('simple-audio-card')
    assert cards == ['/rk817-sound'], 'keep the single existing sound card'
    card = cards[0]
    assert 'status' not in props[card] or get(card, 'status') == 'okay', 'card disabled'
    assert get(card, 'simple-audio-card,name') == 'rk817_int', 'preserve card identity'
    assert cells(card, 'simple-audio-card,aux-devs') == [cells(amp, 'phandle')[0]], \
        'card must bind amplifier auxiliary component'
    assert get(card, 'simple-audio-card,format') == 'i2s'
    assert cells(card, 'simple-audio-card,mclk-fs') == [256], 'preserve 256x MCLK'
    gpio2 = cells('/pinctrl/gpio@ff260000', 'phandle')[0]
    assert cells(card, 'simple-audio-card,hp-det-gpios') == [gpio2, 22, 0], \
        'preserve GPIO2 PC6 active-high headphone detection'
    assert strings(card, 'simple-audio-card,widgets') == [
        'Microphone', 'Mic Jack', 'Headphone', 'Headphones', 'Speaker', 'Internal Speakers',
    ], 'preserve mic/headphones and exact Internal Speakers endpoint'
    assert strings(card, 'simple-audio-card,routing') == ROUTES, \
        'complete two-channel HP-to-amp route and existing mic route required; no SPKO'
    assert strings(card, 'simple-audio-card,pin-switches') == ['Internal Speakers'], \
        'Internal Speakers Switch is the native UCM contract'
    for endpoint, target in [('codec', '/i2c@ff180000/pmic@20'), ('cpu', '/i2s@ff070000')]:
        assert cells(card + '/simple-audio-card,' + endpoint, 'sound-dai') == [
            cells(target, 'phandle')[0]
        ], 'preserve existing RK817/I2S1 DAIs'
    assert not any('gpio-hog' in props[node] for node in nodes), 'no GPIO hogs'
    assert not any(node.endswith('/charger') for node in nodes), 'battery charger remains absent'
    assert not compatible('simple-battery'), 'battery profile remains absent'
    assert len(compatible('rocknix,generic-dsi')) == 1, 'preserve generic DSI'
    print('Compiled DTB: native PA7 amplifier, both HP channels, endpoint switch, '
          'rk817_int, mic and jack preserved. Not driver or playback acceptance.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dtb', type=Path)
    check(parser.parse_args().dtb)


if __name__ == '__main__':
    main()
