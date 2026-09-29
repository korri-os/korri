#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#nix nixpkgs#patch --command python3
"""Off-device static gates for the unregistered Sunshine rotation prototype."""
import json
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile

service = Path(__file__).resolve().parent
if len(sys.argv) != 3:
    raise SystemExit(f'usage: {sys.argv[0]} /nix/store/...-sunshine-source /nix/store/...-baseline-korri-source')
source = Path(sys.argv[1]).resolve()
baseline = Path(sys.argv[2]).resolve()
if not (source / 'src/platform/linux/kmsgrab.cpp').is_file():
    raise SystemExit('Sunshine source path is missing')
approved = baseline / 'services/sunshine/approved-patches.nix'
if not approved.is_file() or not (baseline / 'plugins/sunshine/korri-sunshine-input-seat-receiver.service').is_file():
    raise SystemExit('The installed Mini plugin producer is missing')
expr = f'let a = import {approved}; in map (p: toString p.path) (a.patches ++ a.rkmppPatches)'
records = json.loads(subprocess.check_output(['nix', 'eval', '--json', '--impure', '--expr', expr], text=True))
probe = service / 'rotation-probe.patch'
probe_files = {line[6:].split('\t', 1)[0] for line in probe.read_text().splitlines() if line.startswith('+++ b/')}
assert probe_files == {
    'src/platform/linux/kmsgrab.cpp',
    'src/platform/linux/wayland.cpp',
    'src/platform/linux/wayland.h',
    'src/video.cpp',
}, 'prototype modified a capture or conversion backend outside its reviewed scope'
assert '+          gl::ctx.Finish(' not in probe.read_text(), 'new GPU completion wait'
paths = set()
for name in records + [str(probe)]:
    for line in Path(name).read_text().splitlines():
        if line.startswith(('--- a/', '+++ b/')):
            relative = line[6:].split('\t', 1)[0]
            if Path(relative).is_absolute() or '..' in Path(relative).parts:
                raise SystemExit(f'unsafe patch path: {relative}')
            paths.add(relative)

# The same probe must apply after the CUDA/software set and after the RKMPP set.
base_expr = f'let a = import {approved}; in map (p: toString p.path) a.patches'
base_records = json.loads(subprocess.check_output(['nix', 'eval', '--json', '--impure', '--expr', base_expr], text=True))
for profile, patches in [('base', base_records), ('rkmpp', records)]:
    with tempfile.TemporaryDirectory(prefix=f'sunshine-rotation-{profile}-') as temp:
        tree = Path(temp)
        for relative in paths:
            original = source / relative
            target = tree / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            if original.is_file():
                shutil.copyfile(original, target)
                target.chmod(target.stat().st_mode | stat.S_IWUSR)
        for name in patches:
            result = subprocess.run(['patch', '-p1', '--batch', '--forward', '-i', name], cwd=tree, capture_output=True, text=True)
            if result.returncode:
                raise SystemExit(f'{profile}: approved patch {Path(name).name} failed:\n{result.stdout}\n{result.stderr}')
        kms = tree / 'src/platform/linux/kmsgrab.cpp'
        before = kms.read_text()
        result = subprocess.run(['patch', '-p1', '--batch', '--forward', '--fuzz=0', '-i', str(probe)], cwd=tree, capture_output=True, text=True)
        if result.returncode:
            raise SystemExit(f'{profile}: prototype patch failed:\n{result.stdout}\n{result.stderr}')
        after = kms.read_text()
        vram_marker = 'class display_vram_t: public display_t {'
        gpu_end = '}  // namespace kms'
        before_gpu = before.split(vram_marker, 1)[1].split(gpu_end, 1)[0]
        after_gpu = after.split(vram_marker, 1)[1].split(gpu_end, 1)[0]
        assert before_gpu == after_gpu, 'KMS GPU capture route changed'
        original_readback = 'gl::ctx.GetTextureSubImage(rgb->tex[0], 0, img_offset_x, img_offset_y, 0, width, height, 1, GL_BGRA, GL_UNSIGNED_BYTE, img_out->height * img_out->row_pitch, img_out->data);'
        assert before.count(original_readback) == after.count(original_readback) == 1, 'unrotated readback changed'
        assert after.count('gl::ctx.GetTextureSubImage(') == before.count('gl::ctx.GetTextureSubImage(') + 1, 'extra readback outside rotated path'
        assert 'if (capture_rotate_90) {' in after and 'blend_rotated_cursor(*img_out);' in after
        assert 'blend_cursor(*img_out);' in after, 'unrotated cursor blending changed'
        assert 'cannot capture a visible hardware cursor' not in after
        assert 'auto target_x = captured_cursor.y - img_offset_y + static_cast<std::int32_t>(y);' in after
        assert 'auto target_y = img.height - 1 - (captured_cursor.x - img_offset_x + static_cast<std::int32_t>(x));' in after
        assert 'std::memcpy(&cursor_pixel, captured_cursor.pixels.data()' in after
        assert '#if defined(SUNSHINE_BUILD_WAYLAND) && !defined(SUNSHINE_CAPTURE_ROTATION_FORCE_OFF)' in after
        assert 'vec2(tex.y, 1.0 - tex.x)' in after, 'rotation shader changed: retest orientation'
        assert 'output_transform = monitor->second.output_transform' in after
        assert 'output_name = monitor->second.output_name' in after
        assert 'monitor_descriptor.output_transform.reset();' in after, 'mismatched Wayland mode must not authorize rotation'
        assert '" CRTC="sv << crtc_id << " plane="sv << plane_id' in after
        assert after.count('KMS rotation probe output match: Wayland output=') == 1
        assert after.count('KMS rotation probe decision: Wayland output=') == 1
        assert 'auto plane_rotation = card.get_panel_orientation(plane_id);' in after
        assert 'if (output_transform == WL_OUTPUT_TRANSFORM_90 && plane_rotation == DRM_MODE_ROTATE_0) {' in after
        assert after.index('KMS rotation probe decision: Wayland output=') < after.index('if (output_transform == WL_OUTPUT_TRANSFORM_90 && plane_rotation == DRM_MODE_ROTATE_0) {')
        assert after.index('KMS rotation probe output match: Wayland output=') < after.index('monitor_descriptor.output_transform.reset();')
        print(f'{profile}: approved patches + probe apply; KMS GPU suffix and identity readback unchanged')

# Asymmetric, labelled source: verify the shader's counter-clockwise coordinates.
source_pixels = [['A', 'B', 'C'], ['D', 'E', 'F']]
rotated = [[source_pixels[x][2 - y] for x in range(2)] for y in range(3)]
assert rotated == [['C', 'F'], ['B', 'E'], ['A', 'D']]
# Verify cursor sample placement including a partially clipped cursor. The
# shader and cursor must use the same source-to-destination mapping.
for source_width, source_height, cursor_x, cursor_y, cursor_width, cursor_height in [
    (3, 2, 0, 0, 3, 2),
    (3, 2, -1, 0, 3, 2),
    (3, 2, 2, 1, 2, 2),
]:
    for y in range(cursor_height):
        for x in range(cursor_width):
            raw_x, raw_y = cursor_x + x, cursor_y + y
            target_x, target_y = raw_y, source_width - 1 - raw_x
            visible = 0 <= raw_x < source_width and 0 <= raw_y < source_height
            assert visible == (0 <= target_x < source_height and 0 <= target_y < source_width)
            if visible:
                assert rotated[target_y][target_x] == source_pixels[raw_y][raw_x]
print('static gates passed (no runtime GPU, cursor compositing, or device timing claim)')
