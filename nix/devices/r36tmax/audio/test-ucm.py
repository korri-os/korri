#!/usr/bin/env python3
"""Parse the shipped UCM with libasound; inspect policy without opening any card."""
import argparse
import ctypes
from pathlib import Path


def check(text, library):
    alsa = ctypes.CDLL(str(library))
    pointer = ctypes.c_void_p
    alsa.snd_config_load_string.argtypes = [ctypes.POINTER(pointer), ctypes.c_char_p, ctypes.c_size_t]
    alsa.snd_config_load_string.restype = ctypes.c_int
    alsa.snd_config_search.argtypes = [pointer, ctypes.c_char_p, ctypes.POINTER(pointer)]
    alsa.snd_config_search.restype = ctypes.c_int
    alsa.snd_config_get_string.argtypes = [pointer, ctypes.POINTER(ctypes.c_char_p)]
    alsa.snd_config_get_string.restype = ctypes.c_int
    alsa.snd_config_delete.argtypes = [pointer]
    alsa.snd_config_delete.restype = ctypes.c_int
    config = pointer()
    data = text.encode()
    assert alsa.snd_config_load_string(ctypes.byref(config), data, len(data)) == 0, 'native UCM syntax'
    try:
        def string(path):
            node, value = pointer(), ctypes.c_char_p()
            assert alsa.snd_config_search(config, path.encode(), ctypes.byref(node)) == 0, path
            assert alsa.snd_config_get_string(node, ctypes.byref(value)) == 0, path
            return value.value.decode()

        condition = 'If.1.'
        assert string(condition + 'Condition.Type') == 'ControlExists'
        assert string(condition + 'Condition.Control') == "name='Internal Speakers Switch'"
        enabled = condition + 'True.'
        assert string(enabled + 'Define.pbk_mux') == 'HP', 'external amp must use HP'
        for device, sequence, state in [
            ('Speaker', 'EnableSequence', 'on'),
            ('Speaker', 'DisableSequence', 'off'),
            ('Headphones', 'EnableSequence', 'off'),
        ]:
            path = enabled + f'SectionDevice.{device}.{sequence}.'
            assert string(path + '0') == 'cset', path
            assert string(path + '1') == f"name='Internal Speakers Switch' {state}", path
        assert string('SectionDevice.Headphones.EnableSequence.1') == "name='Playback Mux' HP"
        assert string('SectionDevice.Speaker.EnableSequence.1') == "name='Playback Mux' ${var:pbk_mux}"
        assert string('SectionDevice.Headphones.Value.JackControl') == 'Headphones Jack'
        assert string('SectionDevice.Headphones.ConflictingDevice.0') == 'Speaker'
        assert string('SectionDevice.Speaker.ConflictingDevice.0') == 'Headphones'
        assert string('SectionDevice.Mic.Value.CapturePCM') == 'hw:${CardId}'
    finally:
        alsa.snd_config_delete(config)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('ucm2', type=Path)
    parser.add_argument('libasound', type=Path)
    args = parser.parse_args()
    master = args.ucm2 / 'Rockchip/rk817-sound/rk817-sound.conf'
    assert (args.ucm2 / 'conf.d/simple-card/rk817_int.conf').resolve() == master.resolve()
    assert 'File "/Rockchip/rk817-sound/HiFi.conf"' in master.read_text()
    text = (master.parent / 'HiFi.conf').read_text()
    check(text, args.libasound)
    print('Native ALSA parser: rk817_int profile, conditional HP route and headphone-off guard pass.')
    guard = '''\t\tSectionDevice."Headphones".EnableSequence [
\t\t\tcset "name='Internal Speakers Switch' off"
\t\t]'''
    assert text.count(guard) == 1
    for name, candidate in [
        ('missing boot-with-headphones guard', text.replace(guard, '')),
        ('headphones leave speaker on', text.replace(guard, guard.replace("Switch' off", "Switch' on"))),
        ('wrong external amp mux', text.replace('Define.pbk_mux "HP"', 'Define.pbk_mux "SPK"')),
        ('switch name mismatch', text.replace('Internal Speakers Switch', 'Speaker Switch')),
    ]:
        try:
            check(candidate, args.libasound)
        except AssertionError:
            print(f'PASS negative UCM control: {name} refused')
        else:
            raise AssertionError(f'{name} was accepted')
    print('4 UCM mutations refused. No ControlExists evaluation, mixer writes or playback performed.')


if __name__ == '__main__':
    main()
