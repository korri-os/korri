# The console: tty1 autologins `simon` into sway. One borderless foot window
# fills the screen and runs tmux. wvkbd is the on-screen keyboard (swipe up
# from the bottom edge, or Super+k). No bar, no display manager, no desktop.
#
# The output is the msm KMS device driving the panel over bonded DSI
# (../kernel: dts, drivers/panel-samsung-ana38401.c). simpledrm shows the
# bootloader framebuffer until msm binds and takes it over. The panel has one
# mode at scale 1; size comes from the font. Brightness is the panel's
# hardware level (/sys/class/backlight/*/brightness, 0 to 45, AOR steps);
# blanking is DPMS (the panel's display off).
{
  config,
  lib,
  pkgs,
  ...
}:
let
  osk = pkgs.writeShellApplication {
    name = "tabs6-osk";
    runtimeInputs = [ pkgs.procps ];
    # wvkbd: SIGUSR1 hides, SIGUSR2 shows, SIGRTMIN toggles.
    text = ''
      case "''${1:-toggle}" in
      show) pkill -USR2 -x wvkbd-mobintl ;;
      hide) pkill -USR1 -x wvkbd-mobintl ;;
      *) pkill -RTMIN -x wvkbd-mobintl ;;
      esac
    '';
  };

  dim = pkgs.writeShellApplication {
    name = "tabs6-dim";
    runtimeInputs = [
      config.programs.sway.package
      pkgs.coreutils
      pkgs.jq
    ];
    text = ''
      # Hardware brightness (the panel's backlight level) and DPMS blanking.
      # simon may write the level: see the udev rule below.
      bl=
      for d in /sys/class/backlight/*; do bl=$d; break; done
      [ -n "$bl" ] || { echo "tabs6-dim: no backlight device" >&2; exit 1; }
      max=$(cat "$bl/max_brightness")
      cur=$(cat "$bl/brightness")
      # The level before the idle dim, while it is in force.
      saved="''${XDG_RUNTIME_DIR:-/tmp}/tabs6-dim.saved"
      # Top-edge swipe: next level up from here, wrapping to the lowest.
      cycle=(2 12 24 45)
      step=4
      set_level() {
        local l=$1
        [ "$l" -lt 0 ] && l=0
        [ "$l" -gt "$max" ] && l=$max
        echo "$l" > "$bl/brightness"
      }
      case "''${1:-cycle}" in
      up) set_level $((cur + step)) ;;
      down) set_level $((cur - step)) ;;
      cycle)
        for l in "''${cycle[@]}"; do
          if [ "$l" -gt "$cur" ]; then set_level "$l"; exit 0; fi
        done
        set_level "''${cycle[0]}"
        ;;
      idle) [ -e "$saved" ] || echo "$cur" > "$saved"; set_level 0 ;;
      wake) if [ -e "$saved" ]; then set_level "$(cat "$saved")"; rm -f "$saved"; fi ;;
      blank) swaymsg -q 'output * power off' ;;
      unblank) swaymsg -q 'output * power on' ;;
      toggle)
        if swaymsg -t get_outputs -r | jq -e 'any(.[]; .power)' > /dev/null; then
          swaymsg -q 'output * power off'
        else
          swaymsg -q 'output * power on'
        fi
        ;;
      *) echo "usage: tabs6-dim up|down|cycle|idle|wake|blank|unblank|toggle" >&2; exit 2 ;;
      esac
    '';
  };

  terminal = pkgs.writeShellApplication {
    name = "tabs6-terminal";
    runtimeInputs = [
      pkgs.foot
      pkgs.tmux
    ];
    # If foot exits, start it again; the tmux session survives either way.
    text = ''
      while :; do
        foot tmux new-session -A -s main || sleep 1
      done
    '';
  };

  swayConfig = pkgs.writeText "sway-config" ''
    set $mod Mod4

    output * bg #000000 solid_color
    # sway would pick scale 2 from the panel's 227x142 mm; the font sets size.
    output * scale 1
    default_border none
    default_floating_border none
    hide_edge_borders both
    gaps inner 0
    focus_follows_mouse no
    seat * hide_cursor 3000

    input type:touch events enabled

    exec tabs6-terminal
    exec wvkbd-mobintl --hidden -L 520 -H 760
    exec lisgd -d /dev/input/touchscreen -w 2560 -h 1600 \
      -g '1,DU,B,*,R,tabs6-osk toggle' \
      -g '1,UD,T,*,R,tabs6-dim cycle'
    # 2 min idle: lowest level; 5 min: DPMS off. Input undoes both.
    exec swayidle -w \
      timeout 120 'tabs6-dim idle' resume 'tabs6-dim wake' \
      timeout 300 'swaymsg "output * power off"' resume 'swaymsg "output * power on"'

    bindsym $mod+k exec tabs6-osk toggle
    # Short press: DPMS off or on. logind ignores it, so the tablet never
    # suspends from a tap; a long press still powers off.
    bindsym XF86PowerOff exec tabs6-dim toggle
    bindsym $mod+F1 exec tabs6-dim down
    bindsym $mod+F2 exec tabs6-dim up
    bindsym $mod+Return exec foot tmux new-session -A -s main
    bindsym $mod+Shift+q kill
    bindsym XF86AudioRaiseVolume exec wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+
    bindsym XF86AudioLowerVolume exec wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-
    bindsym XF86AudioMute exec wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle
  '';
in
lib.mkIf config.tabs6.console.enable {
  users.users.simon = {
    isNormalUser = true;
    uid = 1000;
    extraGroups = [
      "wheel"
      "video"
      "render"
      "input"
      "audio"
    ];
  };
  # simon has no password; root over SSH is the admin path, and sudo works
  # from the console without one.
  security.sudo.wheelNeedsPassword = false;

  services.getty.autologinUser = "simon";
  environment.loginShellInit = ''
    if [ "$(tty)" = /dev/tty1 ] && [ "$USER" = simon ] && [ -z "''${WAYLAND_DISPLAY:-}" ]; then
      # Wait for msm: its render node appears only when the whole display
      # pipe (DPU, both DSI links, panel) and the GPU have bound. Starting on
      # simpledrm first would lose the output when msm takes it over. After
      # 30 s sway starts anyway, on whatever DRM device exists.
      for _ in $(seq 1 300); do [ -e /dev/dri/renderD128 ] && break; sleep 0.1; done
      exec sway
    fi
  '';

  programs.sway = {
    enable = true;
    xwayland.enable = false;
    wrapperFeatures.gtk = false;
    extraPackages = [ ];
  };
  environment.etc."sway/config".source = swayConfig;

  environment.etc."xdg/foot/foot.ini".text = ''
    [main]
    font=DejaVu Sans Mono:pixelsize=34
    pad=8x8
    [colors-dark]
    background=000000
  '';

  programs.tmux = {
    enable = true;
    extraConfig = ''
      set -g mouse on
      set -g history-limit 50000
    '';
  };

  fonts.packages = [ pkgs.dejavu_fonts ];

  environment.systemPackages = [
    osk
    dim
    terminal
    pkgs.foot
    pkgs.wvkbd
    pkgs.lisgd
    pkgs.swayidle
  ];

  # A stable name for the touchscreen, for lisgd.
  # simon (group video) sets the panel level with tabs6-dim.
  services.udev.extraRules = ''
    SUBSYSTEM=="input", KERNEL=="event*", ENV{ID_INPUT_TOUCHSCREEN}=="1", SYMLINK+="input/touchscreen"
    ACTION=="add", SUBSYSTEM=="backlight", RUN+="${pkgs.coreutils}/bin/chgrp video /sys/class/backlight/%k/brightness", RUN+="${pkgs.coreutils}/bin/chmod g+w /sys/class/backlight/%k/brightness"
  '';

  # hardware.graphics and the msm module serve both systems: device.nix.
}
