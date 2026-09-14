#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 dtc
"""Check button binding composition in a compiled DTB, not physical operation."""
import argparse
from pathlib import Path
import re
import subprocess


# ROCKNIX/distribution 697d112a64e442e79e3d27b064351d917657301a:
# projects/ROCKNIX/devices/RK3326/linux/dts/rockchip/rk3326-gameconsole-eeclone.dts
# lines 177-265 (GPIO/code pairs), 819-841 (GPIO mode and plain pull-ups).
# Codes are Linux input-event-codes.h values, not ROCKNIX's numeric comments.
EXPECTED = {
    (3, 11): 0x137,  # PB3 BTN_TR
    (3, 12): 0x139,  # PB4 BTN_TR2
    (3, 13): 0x136,  # PB5 BTN_TL
    (3, 14): 0x138,  # PB6 BTN_TL2
    (3, 16): 0x131,  # PC0 BTN_EAST (source label A)
    (3, 17): 0x130,  # PC1 BTN_SOUTH (B)
    (3, 18): 0x133,  # PC2 BTN_NORTH (X)
    (3, 19): 0x134,  # PC3 BTN_WEST (Y)
    (3, 20): 0x222,  # PC4 BTN_DPAD_LEFT
    (3, 21): 0x223,  # PC5 BTN_DPAD_RIGHT
    (3, 22): 0x220,  # PC6 BTN_DPAD_UP
    (3, 23): 0x221,  # PC7 BTN_DPAD_DOWN
    (3, 24): 0x13A,  # PD0 BTN_SELECT
    (3, 25): 0x13B,  # PD1 BTN_START
    (2, 13): 0x13D,  # PB5 BTN_THUMBL
    (2, 14): 0x13E,  # PB6 BTN_THUMBR
    (3, 10): 0x13C,  # PB2 BTN_MODE (Fn)
}
EXCLUDED = {(3, 26), (3, 27), (3, 6), (3, 15), (2, 15)}


def check(dtb):
    def get(node, prop, kind=None):
        args = ['fdtget'] + (['-t', kind] if kind else [])
        return subprocess.check_output(args + [str(dtb), node, prop], text=True).strip()

    def listing(node, option):
        return subprocess.check_output(
            ['fdtget', option, str(dtb), node], text=True,
        ).split()

    def cells(node, prop):
        return [int(cell) for cell in get(node, prop, 'u').split()]

    # Resolve references by their compiled phandles, not label names or numbers
    # that happen to be assigned by one version of dtc.
    nodes = []
    pending = ['/']
    props = {}
    phandles = {}
    while pending:
        node = pending.pop()
        nodes.append(node)
        props[node] = set(listing(node, '-p'))
        if 'phandle' in props[node]:
            phandles[cells(node, 'phandle')[0]] = node
        pending.extend(node.rstrip('/') + '/' + child for child in listing(node, '-l'))

    buttons = [node for node in nodes if 'compatible' in props[node]
               and get(node, 'compatible') == 'gpio-keys-polled']
    assert len(buttons) == 1, 'expected one native gpio-keys-polled button device'
    node = buttons[0]
    assert props[node] <= {
        'compatible', 'poll-interval', 'pinctrl-names', 'pinctrl-0', 'status', 'phandle',
    }, 'button device must retain native identity and button-only properties'
    assert 'status' not in props[node] or get(node, 'status') == 'okay'
    assert cells(node, 'poll-interval') == [10], 'ROCKNIX polling policy is 10 ms'
    assert get(node, 'pinctrl-names') == 'default'

    providers = {}
    for bank, address in [(2, 'ff260000'), (3, 'ff270000')]:
        provider = f'/pinctrl/gpio@{address}'
        assert get(provider, 'compatible') == 'rockchip,gpio-bank'
        assert 'gpio-controller' in props[provider]
        assert cells(provider, '#gpio-cells') == [2]
        providers[cells(provider, 'phandle')[0]] = bank

    children = listing(node, '-l')
    assert len(children) == 17, 'expected exactly 17 game buttons'
    actual = {}
    for child in children:
        assert re.fullmatch(r'button-[a-z0-9-]+', child), 'use native button child names'
        path = node + '/' + child
        assert props[path] <= {'gpios', 'linux,code', 'label', 'phandle'}, \
            'use native EV_KEY and debounce defaults; no axes, wakeup or custom properties'
        gpio = cells(path, 'gpios')
        assert len(gpio) == 3 and gpio[0] in providers, f'{child}: GPIO2/3 required'
        assert gpio[2] == 1, f'{child}: expected active-low'
        pin = (providers[gpio[0]], gpio[1])
        assert pin not in EXCLUDED, f'{child}: excluded volume, motor, reset or mux pin'
        assert pin not in actual, f'{child}: duplicate GPIO'
        code = cells(path, 'linux,code')
        assert len(code) == 1
        actual[pin] = code[0]
    assert actual == EXPECTED, f'GPIO/code mapping mismatch: {actual}'

    pins = []
    for reference in cells(node, 'pinctrl-0'):
        group = phandles[reference]
        entries = cells(group, 'rockchip,pins')
        assert len(entries) % 4 == 0
        for offset in range(0, len(entries), 4):
            bank, pin, function, config = entries[offset:offset + 4]
            assert (bank, pin) not in EXCLUDED, 'excluded pin in button pinctrl'
            assert function == 0, 'button pin must use GPIO mode'
            pull = phandles[config]
            assert props[pull] == {'bias-pull-up', 'phandle'}, 'expected plain pull-up'
            pins.append((bank, pin))
    assert len(pins) == 17 and set(pins) == set(EXPECTED), \
        'pinctrl must cover exactly the same 17 game pins'
    print('Compiled DTB: 17 active-low game buttons, exact GPIO/code map, '
          'plain pull-ups, native identity and 10 ms polling. Not physical acceptance.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dtb', type=Path)
    check(parser.parse_args().dtb)


if __name__ == '__main__':
    main()
