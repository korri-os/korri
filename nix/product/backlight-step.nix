# Harvested unchanged from legacy:product/systems/nixos/modules/korri-input.nix
# (korriBacklightStep, 8ebddeb90). Steps every backlight under
# /sys/class/backlight by a percentage of its max_brightness, never below 1.
{ pkgs }:
pkgs.writeShellApplication {
  name = "korri-backlight-step";
  runtimeInputs = with pkgs; [ coreutils ];
  text = ''
    usage() {
      echo "usage: korri-backlight-step [+N|-N][%]" >&2
    }

    step="''${1:-}"
    case "$step" in
      +*|-*) ;;
      *) usage; exit 64 ;;
    esac

    sign="''${step:0:1}"
    amount="''${step:1}"
    amount="''${amount%\%}"
    if [ -z "$amount" ]; then
      usage
      exit 64
    fi
    case "$amount" in
      *[!0-9]*) usage; exit 64 ;;
    esac

    backlight_root="''${KORRI_BACKLIGHT_ROOT:-/sys/class/backlight}"
    adjusted=0
    found=0

    for device in "$backlight_root"/*; do
      [ -d "$device" ] || continue
      found=1

      brightness_file="$device/brightness"
      max_file="$device/max_brightness"
      device_name="$(basename "$device")"

      if [ ! -r "$brightness_file" ] || [ ! -r "$max_file" ] || [ ! -w "$brightness_file" ]; then
        echo "korri-backlight-step: skipping $device_name; brightness is not readable/writable" >&2
        continue
      fi

      current="$(cat "$brightness_file")"
      max="$(cat "$max_file")"
      case "$current:$max" in
        *[!0-9:]*|:*)
          echo "korri-backlight-step: skipping $device_name; non-numeric brightness" >&2
          continue
          ;;
      esac
      if [ "$max" -le 0 ]; then
        echo "korri-backlight-step: skipping $device_name; max_brightness is $max" >&2
        continue
      fi

      delta=$(((max * amount + 99) / 100))
      if [ "$delta" -lt 1 ]; then
        delta=1
      fi

      if [ "$sign" = "+" ]; then
        next=$((current + delta))
      else
        next=$((current - delta))
      fi

      if [ "$next" -lt 1 ]; then
        next=1
      fi
      if [ "$next" -gt "$max" ]; then
        next="$max"
      fi

      if printf '%s\n' "$next" >"$brightness_file"; then
        adjusted=$((adjusted + 1))
        echo "korri-backlight-step: $device_name $current -> $next / $max" >&2
      else
        echo "korri-backlight-step: failed to write $device_name" >&2
      fi
    done

    if [ "$found" -eq 0 ]; then
      echo "korri-backlight-step: no backlight devices under $backlight_root" >&2
      exit 1
    fi
    if [ "$adjusted" -eq 0 ]; then
      echo "korri-backlight-step: no writable backlight devices adjusted" >&2
      exit 1
    fi
  '';
}
