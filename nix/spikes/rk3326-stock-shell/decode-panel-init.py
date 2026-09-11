#!/usr/bin/env python3
"""Decode a Rockchip vendor `panel-init-sequence` into readable DSI commands.

The vendor device tree stores the panel bring-up as one opaque hex blob.
Mainline's panel drivers express the same thing as a list of DCS writes, so
the blob has to be unpacked before the two can be compared or a new panel
variant written.

Layout of each command, repeated until the blob is consumed:

    <data_type> <delay_ms> <payload_len> <payload ...>

where payload[0] is the DCS command and the rest are its parameters.
"""

import re
import sys

# MIPI DSI data types that appear in these blobs.
DATA_TYPES = {
    0x05: "DCS short write, 0 param",
    0x15: "DCS short write, 1 param",
    0x39: "DCS long write",
}

# ST7703 register names, from the mainline driver's own definitions, so the
# decoded output can be read against it directly.
ST7703_COMMANDS = {
    0xB9: "SETEXTC",
    0xB1: "SETPOWER",
    0xB2: "SETDISP",
    0xB3: "SETRGBIF",
    0xB4: "SETCYC",
    0xB5: "SETBGP",
    0xB6: "SETVCOM",
    0xB8: "SETPOWER_EXT",
    0xBA: "SETMIPI",
    0xBC: "SETVDC",
    0xBF: "SETSCR / UNKNOWN_BF",
    0xC0: "SETSCR",
    0xC1: "SETPANEL/POWER",
    0xC6: "UNKNOWN_C6",
    0xC7: "SETGAMMA?",
    0xC8: "UNKNOWN_C8",
    0xCC: "SETPANEL",
    0xE0: "SETGAMMA",
    0xE3: "SETEQ",
    0xE9: "SETGIP1",
    0xEA: "SETGIP2",
    0xEF: "UNKNOWN_EF",
    0x11: "EXIT_SLEEP_MODE",
    0x29: "SET_DISPLAY_ON",
    0x28: "SET_DISPLAY_OFF",
    0x10: "ENTER_SLEEP_MODE",
}


def extract_blob(dts_text: str) -> bytes:
    match = re.search(r"panel-init-sequence = \[([0-9a-fA-F\s]+)\]", dts_text)
    if match is None:
        sys.exit("no panel-init-sequence found")
    return bytes.fromhex("".join(match.group(1).split()))


def decode(blob: bytes) -> None:
    index = 0
    count = 0
    while index + 3 <= len(blob):
        data_type = blob[index]
        delay_ms = blob[index + 1]
        length = blob[index + 2]
        payload = blob[index + 3 : index + 3 + length]
        index += 3 + length
        count += 1

        if not payload:
            continue

        command = payload[0]
        params = payload[1:]
        name = ST7703_COMMANDS.get(command, "")
        type_name = DATA_TYPES.get(data_type, f"type 0x{data_type:02x}")

        head = f"{count:3d}. 0x{command:02X} {name:<16}"
        tail = " ".join(f"{b:02x}" for b in params)
        delay = f"  (+{delay_ms}ms)" if delay_ms else ""
        print(f"{head} [{len(params):2d}] {tail}{delay}")
        print(f"     {type_name}")

    if index != len(blob):
        print(f"\nWARNING: {len(blob) - index} trailing bytes did not parse")
    else:
        print(f"\n{count} commands, {len(blob)} bytes, fully parsed")


if __name__ == "__main__":
    with open(sys.argv[1]) as handle:
        decode(extract_blob(handle.read()))
