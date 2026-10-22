# Pass the approved korri-plugins sunshine-v4l2m2m-probe derivation.
# Keep its prepared FFmpeg, compiler, dependencies and build recipe unchanged.
{ probe }:
probe.overrideAttrs {
  src = ./ffmpeg-probe.c;
  version = "h700-acceptance";
}
