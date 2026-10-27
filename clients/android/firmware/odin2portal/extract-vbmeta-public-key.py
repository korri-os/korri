#!/usr/bin/env python3
"""Extract the encoded AVB public-key blob from a vbmeta image."""

import hashlib
import struct
import sys
from pathlib import Path

HEADER_SIZE = 256


def extract_public_key(image: bytes) -> bytes:
    if len(image) < HEADER_SIZE or image[:4] != b"AVB0":
        raise ValueError("not an AVB vbmeta image")

    authentication_size = struct.unpack_from(">Q", image, 12)[0]
    auxiliary_size = struct.unpack_from(">Q", image, 20)[0]
    public_key_offset = struct.unpack_from(">Q", image, 64)[0]
    public_key_size = struct.unpack_from(">Q", image, 72)[0]
    auxiliary_start = HEADER_SIZE + authentication_size
    auxiliary_end = auxiliary_start + auxiliary_size

    if auxiliary_end > len(image):
        raise ValueError("invalid AVB auxiliary-block bounds")
    if public_key_size == 0 or public_key_offset + public_key_size > auxiliary_size:
        raise ValueError("invalid AVB public-key bounds")

    start = auxiliary_start + public_key_offset
    return image[start : start + public_key_size]


def main(arguments: list[str]) -> int:
    if len(arguments) != 2:
        raise SystemExit("usage: extract-vbmeta-public-key.py <vbmeta-image> <output>")

    source = Path(arguments[0])
    output = Path(arguments[1])
    if not source.is_file() or source.is_symlink():
        raise SystemExit(f"source is missing, not regular, or symbolic: {source}")
    if output.exists() or output.is_symlink():
        raise SystemExit(f"output already exists: {output}")

    public_key = extract_public_key(source.read_bytes())
    output.write_bytes(public_key)
    print(f"AVB_PUBLIC_KEY_EXTRACTED sha1={hashlib.sha1(public_key).hexdigest()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
