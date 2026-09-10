{
  pkgs,
  joypadAutoconfig ? pkgs.retroarch-joypad-autoconfig,
}:

let
  # The pinned nixpkgs input ships this exact v1.22.0 baseline. A lock update
  # must fail here until the upstream gameplay bindings are reviewed again.
  upstreamVersion = "1.22.0";
  upstreamFile = "udev/Microsoft X-Box 360 pad.cfg";
  upstreamSha256 = "5765040cd5d00a4f3e4386826077c0a3576c035ecedfc19e44199ab6625962e8";
  transformedSha256 = "1ba2653ed20ad8df20a53dde9c092e2cd92cd540d0275556250eb3a9483e92b1";

  transform =
    {
      upstreamRoot,
      outputRoot,
    }:
    ''
      upstream_config="${upstreamRoot}/${upstreamFile}"
      output_config="${outputRoot}/${upstreamFile}"

      test -f "$upstream_config" || {
        echo "reviewed upstream Xbox 360 RetroArch autoconfig is missing" >&2
        exit 1
      }
      test "$(sha256sum "$upstream_config" | cut -d ' ' -f 1)" = '${upstreamSha256}' || {
        echo "upstream Xbox 360 RetroArch autoconfig changed unexpectedly" >&2
        exit 1
      }
      test "$(grep -Ec '^input_menu_toggle_btn(_label)? =' "$upstream_config")" = 2 \
        && grep -Fx 'input_menu_toggle_btn = "8"' "$upstream_config" >/dev/null \
        && grep -Fx 'input_menu_toggle_btn_label = "Guide"' "$upstream_config" >/dev/null || {
          echo "upstream Xbox 360 Guide/Home binding changed unexpectedly" >&2
          exit 1
        }

      mkdir -p "$(dirname "$output_config")"
      sed '/^input_menu_toggle_btn\(_label\)\? =/d' "$upstream_config" > "$output_config"

      test "$(sha256sum "$output_config" | cut -d ' ' -f 1)" = '${transformedSha256}' || {
        echo "Xbox 360 RetroArch autoconfig transform produced unexpected output" >&2
        exit 1
      }
      if grep -Eq '^input_menu_toggle_btn(_label)? =' "$output_config"; then
        echo "Xbox 360 RetroArch autoconfig still captures Guide/Home" >&2
        exit 1
      fi
    '';
in
assert joypadAutoconfig.version == upstreamVersion;
pkgs.runCommand "korri-inputplumber-retroarch-autoconfig-${upstreamVersion}"
  {
    passthru = {
      inherit
        transform
        upstreamFile
        upstreamSha256
        transformedSha256
        ;
      upstream = joypadAutoconfig;
    };
  }
  ''
    set -euo pipefail
    ${transform {
      upstreamRoot = "${joypadAutoconfig}/share/libretro/autoconfig";
      outputRoot = "$out/share/libretro/autoconfig";
    }}
  ''
