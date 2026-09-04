# Chromium benchmarking harness for the RG353M panel.
#
# Korri's portal is a WebView, so browser performance on this hardware is a
# direct product signal rather than a synthetic curiosity. The device has no
# display server, so `cage` provides a single-application Wayland session on
# the panel and Chromium runs inside it.
#
# Panfrost exposes GLES 3.1 and no usable Vulkan, so Chromium is pointed at
# the GL path deliberately. glmark2 showed this GPU is memory-bandwidth bound,
# not shader bound, which is why rasterization and compositing flags matter
# more here than shader-related ones.
{ pkgs, ... }:

let
  panelWidth = 640;
  panelHeight = 480;

  chromiumFlags = [
    "--ozone-platform=wayland"
    "--enable-features=UseOzonePlatform,VaapiVideoDecoder,VaapiVideoDecodeLinuxGL"
    "--use-gl=angle"
    "--use-angle=gles-egl"
    "--enable-gpu-rasterization"
    "--enable-zero-copy"
    "--ignore-gpu-blocklist"
    # 1.9 GB of RAM total, so keep Chromium from over-committing.
    "--renderer-process-limit=2"
    "--disable-dev-shm-usage"
    "--no-first-run"
    "--no-default-browser-check"
    "--disable-features=Translate"
  ];

  browserBench = pkgs.writeShellApplication {
    name = "browser-bench";
    runtimeInputs = with pkgs; [
      cage
      chromium
      coreutils
    ];
    text = ''
      url="''${1:-https://browserbench.org/Speedometer3.0/}"
      export XDG_RUNTIME_DIR="''${XDG_RUNTIME_DIR:-/run/user/0}"
      mkdir -p "$XDG_RUNTIME_DIR"
      chmod 700 "$XDG_RUNTIME_DIR"

      profile="$(mktemp -d)"
      trap 'rm -rf "$profile"' EXIT

      echo "GPU: $(cat /sys/class/devfreq/fde60000.gpu/cur_freq) Hz, governor $(cat /sys/class/devfreq/fde60000.gpu/governor)"
      echo "opening $url on the panel"

      exec cage -- chromium \
        ${builtins.concatStringsSep " \\\n        " chromiumFlags} \
        --user-data-dir="$profile" \
        --window-size=${toString panelWidth},${toString panelHeight} \
        --start-fullscreen \
        "$url"
    '';
  };

  # Report which renderer Chromium actually negotiated. This is the first thing
  # to check when a score looks wrong, because a silent fall back to software
  # rendering looks like a slow GPU rather than a broken one.
  #
  # chrome://gpu cannot be used here: under --headless it redirects to the new
  # tab page and --dump-dom returns that instead. Ask WebGL directly.
  browserGpuReport = pkgs.writeShellApplication {
    name = "browser-gpu-report";
    runtimeInputs = with pkgs; [
      chromium
      coreutils
      gnugrep
    ];
    text = ''
      export XDG_RUNTIME_DIR="''${XDG_RUNTIME_DIR:-/run/user/0}"
      mkdir -p "$XDG_RUNTIME_DIR"
      chmod 700 "$XDG_RUNTIME_DIR"
      profile="$(mktemp -d)"
      trap 'rm -rf "$profile"' EXIT

      # Headless needs no compositor, so cage is deliberately absent here.
      chromium \
        --headless=new \
        --no-sandbox \
        --use-gl=angle \
        --use-angle=gles-egl \
        --enable-gpu-rasterization \
        --ignore-gpu-blocklist \
        --user-data-dir="$profile" \
        --virtual-time-budget=6000 \
        --dump-dom "file://${./browser-probe.html}" 2>/dev/null \
        | grep -o 'RESULT\[[^]]*\]' \
        | head -1
    '';
  };
in
{
  environment.systemPackages = [
    browserBench
    browserGpuReport
    pkgs.chromium
    pkgs.cage
  ];

  # cage needs a seat and a writable runtime dir for the root session.
  security.polkit.enable = true;
  services.seatd.enable = true;
  users.users.root.extraGroups = [ "seat" ];
}
