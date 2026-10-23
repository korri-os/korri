"""Frame-ID treaty: sync, 24-bit ID, CRC, and independently decoded inverse.

Geometry is proportional to the output, not a connector or a fixed crop.
A valid pair with different IDs is a visible capture tear, not an unreadable ID.
"""

from dataclasses import dataclass
import numpy as np

BITS = 36
SYNC = 0xA


@dataclass(frozen=True)
class Clean:
    ident: int


@dataclass(frozen=True)
class Torn:
    upper_id: int
    lower_id: int


@dataclass(frozen=True)
class Unreadable:
    reason: str


Frame = Clean | Torn | Unreadable


def frame_ids(frame: Frame) -> tuple[int, ...]:
    if isinstance(frame, Clean):
        return (frame.ident,)
    if isinstance(frame, Torn):
        return (frame.upper_id, frame.lower_id)
    return ()


def crc(ident: int) -> int:
    value = 0
    for byte in ident.to_bytes(3, "big"):
        value ^= byte
        for _ in range(8):
            value = ((value << 1) ^ (7 if value & 128 else 0)) & 255
    return value


def paint(width: int, height: int, ident: int) -> np.ndarray:
    """Reference renderer for the C source's lossy-codec contract tests."""
    cell = width // (BITS + 4)
    if cell < 2 or height < cell * 6 or not 0 < ident < 1 << 24:
        raise ValueError("output too small or ID outside 24-bit range")
    image = np.full((height, width), 96, dtype=np.uint8)
    x, y = (width - BITS * cell) // 2, height // 4
    code = (SYNC << 32) | (ident << 8) | crc(ident)
    for row in range(2):
        for bit in range(BITS):
            value = 235 if ((code >> (BITS - 1 - bit)) & 1) ^ row else 16
            image[
                y + row * 3 * cell : y + (row * 3 + 2) * cell,
                x + bit * cell : x + (bit + 1) * cell,
            ] = value
    return image


@dataclass(frozen=True)
class Location:
    x: float
    y: float
    cell: float


def read(image: np.ndarray, at: Location) -> Frame:
    h, w = image.shape
    xs = np.array([at.x + (bit + 0.5) * at.cell for bit in range(BITS)], dtype=int)
    ys = [int(at.y), int(at.y + 3 * at.cell)]
    if xs.min() < 0 or xs.max() >= w or min(ys) < 0 or max(ys) >= h:
        return Unreadable("bar outside decoded image")
    values = image[np.array(ys)[:, None], xs]
    decoded = []
    for index, row in enumerate(values):
        if np.min(np.abs(row.astype(float) - 128)) < 45:
            return Unreadable(f"row {index} contrast below decoding margin")
        code = sum(int(v >= 128) << (BITS - 1 - bit) for bit, v in enumerate(row))
        if index:
            code ^= (1 << BITS) - 1
        ident = (code >> 8) & 0xFFFFFF
        if code >> 32 != SYNC:
            return Unreadable(f"row {index} sync failed")
        if not ident or code & 255 != crc(ident):
            return Unreadable(f"row {index} CRC failed")
        decoded.append(ident)
    return (
        Clean(decoded[0]) if decoded[0] == decoded[1] else Torn(decoded[0], decoded[1])
    )


def search(image: np.ndarray) -> Location:
    h, _ = image.shape
    for y in range(0, h, max(1, h // 200)):
        row = image[y] >= 128
        edges = np.flatnonzero(row[1:] != row[:-1]) + 1
        # Sync 1010 has four equal-width runs. The fourth can merge with ID.
        for a, b, c in zip(edges, edges[1:], edges[2:]):
            cell = (c - a) / 2
            if cell < 2 or abs((b - a) - (c - b)) > 1 or row[a]:
                continue
            at = Location(float(a - cell), float(y), cell)
            if not isinstance(read(image, at), Unreadable):
                return at
    raise ValueError(
        "ID bar sync pattern not found; inspect discovered output transform"
    )
