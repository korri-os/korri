"""Stream ffmpeg output; never retain a raw full-screen recording."""

import json
from pathlib import Path
import subprocess
import numpy as np
from bar import Frame, Unreadable, read, search


def decode(video: Path, diagnostic: Path) -> list[Frame]:
    probe = subprocess.run(
        [
            "ffprobe",
            "-v",
            "warning",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "json",
            str(video),
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    if probe.stderr:
        raise ValueError("decoder probe warning: " + probe.stderr)
    stream = json.loads(probe.stdout)["streams"][0]
    width, height = stream["width"], stream["height"]
    size = width * height
    ids: list[Frame] = []
    location = None
    with diagnostic.open("wb") as errors:
        process = subprocess.Popen(
            [
                "ffmpeg",
                "-nostdin",
                "-v",
                "warning",
                "-threads",
                "2",
                "-i",
                str(video),
                "-vf",
                "format=gray",
                "-fps_mode",
                "passthrough",
                "-f",
                "rawvideo",
                "-",
            ],
            stdout=subprocess.PIPE,
            stderr=errors,
        )
        assert process.stdout is not None
        try:
            while data := process.stdout.read(size):
                if len(data) != size:
                    raise ValueError("truncated decoded frame")
                image = np.frombuffer(data, dtype=np.uint8).reshape(height, width)
                if location is None:
                    try:
                        location = search(image)
                    except ValueError:
                        ids.append(Unreadable("sync pattern not found"))
                        continue
                ids.append(read(image, location))
            process.wait(timeout=20)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=10)
            process.stdout.close()
        errors.flush()
        warnings = diagnostic.read_text()
        if process.returncode or warnings:
            raise ValueError("decoder warning/error invalidates run: " + warnings)
    if location is None:
        raise ValueError(
            "ID bar sync pattern not found; inspect discovered output transform"
        )
    return ids
