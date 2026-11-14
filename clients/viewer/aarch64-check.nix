# Starts the aarch64 viewer on the x86_64 build machine under QEMU user
# emulation, as nix/product/published-plugin-checks.nix does for the plugin
# host. It proves the package links and starts; it decodes no picture.
{ pkgs, package }:
let
  ffmpeg = package.passthru.ffmpeg;
  qemu = "${pkgs.qemu}/bin/qemu-aarch64";
in
pkgs.runCommand "korri-viewer-aarch64-check" { } ''
  viewer=${package}/bin/korri-viewer
  # LD_BIND_NOW makes the loader resolve every symbol before main runs.
  ${qemu} -E LD_BIND_NOW=1 "$viewer" --help > help.txt
  cat help.txt
  grep -q '^korri-viewer 0.1.0 (GPL-3.0-or-later)$' help.txt
  grep -q -- '-platform v4l2request' help.txt

  # The libavcodec the loader resolves is the patched FFmpeg's.
  ${qemu} -E LD_TRACE_LOADED_OBJECTS=1 "$viewer" > libraries.txt
  cat libraries.txt
  grep -q '=> ${pkgs.lib.getLib ffmpeg}/lib/libavcodec\.so' libraries.txt
  if grep -q 'not found' libraries.txt; then
    echo 'unresolved library' >&2
    exit 1
  fi
  # That FFmpeg has the V4L2 Request device type.
  ${qemu} ${pkgs.lib.getBin ffmpeg}/bin/ffmpeg -hide_banner -hwaccels > hwaccels.txt
  cat hwaccels.txt
  grep -qx 'v4l2request' hwaccels.txt

  # korrid's handheld arguments parse; the run stops at the missing pin.
  if ${qemu} -U KORRI_MOONLIGHT_SERVER_CERTIFICATE "$viewer" stream -app 'Korri Stream' \
      -platform v4l2request -mapping /unused -keydir /unused -windowed \
      -autowindowresize -input /dev/input/event3 -input /dev/input/event7 \
      -port 47989 192.0.2.1 2> request.txt; then
    exit 1
  fi
  cat request.txt
  grep -q 'KORRI_MOONLIGHT_SERVER_CERTIFICATE is required' request.txt
  # The Qualcomm path is not carried (ADR 0063).
  if ${qemu} "$viewer" stream -platform v4l2m2m -keydir /unused 192.0.2.1 2> m2m.txt; then
    exit 1
  fi
  grep -q 'unsupported platform v4l2m2m' m2m.txt

  mkdir -p "$out"
  cp help.txt libraries.txt hwaccels.txt request.txt m2m.txt "$out/"
''
