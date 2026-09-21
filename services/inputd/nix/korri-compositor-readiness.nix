# Shared Sway socket contract and readiness probe for Linux-host compositions.
{
  lib,
  pkgs,
  runtimeDir,
}:
let
  compositorControlDirectory = "/run/korri-compositor";
  compositorControlSocket = "${compositorControlDirectory}/sway-ipc.sock";
  waylandDisplay = "korri-wayland";
  xwaylandDisplay = ":0";
  xwaylandSocket = "/tmp/.X11-unix/X0";
  xwaylandLock = "/tmp/.X0-lock";
  validAbsolutePath =
    path:
    lib.hasPrefix "/" path
    && path != "/"
    && !(lib.hasInfix "//" path)
    && !(lib.hasInfix "/./" path)
    && !(lib.hasSuffix "/." path)
    && !(lib.hasInfix "/../" path)
    && !(lib.hasSuffix "/.." path)
    && builtins.match ".*[[:space:]].*" path == null;
  compositorStillRunning = ''
    if [ -n "''${MAINPID:-}" ] && ! kill -0 "$MAINPID" 2>/dev/null; then
      echo "compositor exited before it became ready" >&2
      exit 1
    fi
  '';
  waitForCompositor = pkgs.writeShellScript "korri-wait-for-compositor" ''
    set -eu
    output_name="''${KORRI_COMPOSITOR_OUTPUT_NAME:?}"
    output_width="''${KORRI_COMPOSITOR_OUTPUT_WIDTH:?}"
    output_height="''${KORRI_COMPOSITOR_OUTPUT_HEIGHT:?}"
    attempt=0
    while [ "$attempt" -lt 60 ]; do
      ${compositorStillRunning}
      if [ -S ${lib.escapeShellArg "${runtimeDir}/${waylandDisplay}"} ] \
        && [ -S ${lib.escapeShellArg xwaylandSocket} ]; then
        outputs="$(${pkgs.sway}/bin/swaymsg -s ${lib.escapeShellArg compositorControlSocket} -t get_outputs -r 2>/dev/null || true)"
        if printf '%s\n' "$outputs" | ${pkgs.jq}/bin/jq -e \
          --arg name "$output_name" \
          --argjson width "$output_width" \
          --argjson height "$output_height" \
          '.[] | select(.name == $name and .active == true and .current_mode.width == $width and .current_mode.height == $height)' \
          >/dev/null; then
          exit 0
        fi
      fi
      attempt=$((attempt + 1))
      ${pkgs.coreutils}/bin/sleep 0.25
    done
    echo "Sway output $output_name did not become active at ''${output_width}x$output_height" >&2
    exit 1
  '';
in
{
  inherit
    compositorControlDirectory
    compositorControlSocket
    compositorStillRunning
    validAbsolutePath
    waitForCompositor
    waylandDisplay
    xwaylandDisplay
    xwaylandLock
    xwaylandSocket
    ;
}
