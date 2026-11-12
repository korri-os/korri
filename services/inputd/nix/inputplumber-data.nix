{ pkgs }:

let
  inherit (pkgs) lib;
  # Pinned upstream 0.75.2 external gamepad set. These are schema/target
  # expectations, not model-specific Korri behavior. Check the complete set below.
  externalDeviceProfiles = [
    {
      file = "60-8bit_do_pro_2.yaml";
      name = "8BitDo 8BitDo Pro 2";
      upstreamTargets = [
        "xbox-elite"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-LogitechDualAction.yaml";
      name = "Logitech Dual ACtion";
      upstreamTargets = [
        "xbox-elite"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-flydigi_vader_4_pro.yaml";
      name = "flydigi-vader-4-pro";
      upstreamTargets = [
        "deck"
        "touchpad"
      ];
    }
    {
      file = "60-horipad_steam.yaml";
      name = "Horipad Steam";
      upstreamTargets = [
        "hori-steam"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-ps4_gamepad.yaml";
      name = "Sony Interactive Entertainment Wireless Controller";
      upstreamTargets = [
        "ds5"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-ps5_ds-edge_gamepad.yaml";
      name = "Sony Interactive Entertainment DualSense Edge Wireless Controller";
      upstreamTargets = [
        "ds5-edge"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-ps5_ds_gamepad.yaml";
      name = "Sony Interactive Entertainment DualSense Wireless Controller";
      upstreamTargets = [
        "ds5"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-switch_pro.yaml";
      name = "Nintendo Co., Ltd. Pro Controller";
      upstreamTargets = [
        "gamepad"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-xbox_360_gamepad.yaml";
      name = "Microsoft X-Box 360 pad";
      upstreamTargets = [
        "xb360"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-xbox_gamepad.yaml";
      name = "Microsoft X-Box pad";
      upstreamTargets = [
        "xb360"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-xbox_one_bt_gamepad.yaml";
      name = "Microsoft X-Box One pad";
      upstreamTargets = [
        "xbox-series"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-xbox_one_elite_gamepad.yaml";
      name = "Microsoft X-Box One Elite pad";
      upstreamTargets = [
        "xbox-elite"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "60-xbox_one_gamepad.yaml";
      name = "Microsoft X-Box One pad";
      upstreamTargets = [
        "xbox-series"
        "mouse"
        "keyboard"
      ];
    }
    {
      file = "65-generic_gamepad.yaml";
      name = "Generic Gamepad";
      upstreamTargets = [
        "xb360"
        "mouse"
        "keyboard"
      ];
    }
  ];
  # These upstream profiles merge hidraw or auxiliary ABS interfaces. Inputd
  # cannot yet prove that they belong to one gameplay source. Do not let the
  # generic fallback claim only their evdev half (see source_topology.rs).
  deferredDeviceFiles = [
    "60-flydigi_vader_4_pro.yaml"
    "60-horipad_steam.yaml"
    "60-ps4_gamepad.yaml"
    "60-ps5_ds-edge_gamepad.yaml"
    "60-ps5_ds_gamepad.yaml"
    "60-xbox_one_bt_gamepad.yaml"
  ];
  selectedDeviceProfiles = builtins.filter (
    profile: !(builtins.elem profile.file deferredDeviceFiles)
  ) externalDeviceProfiles;
  deferredDeviceProfiles = builtins.filter (
    profile: builtins.elem profile.file deferredDeviceFiles
  ) externalDeviceProfiles;
  # This is the shared immutable mapping profile loaded by inputd for every
  # supported composite, not a filter on the physical controller's name.
  resolvedProfile = "korri-60-xbox_one_gamepad.yaml";
  shortcutMappings = ./inputplumber-korri-dbus-shortcuts.yaml;

  transformSelectedProfiles =
    { inputplumberRoot }:
    ''
      # Fail closed when an upstream update adds/removes an external profile.
      printf '%s\n' ${
        lib.escapeShellArgs (map (profile: profile.file) externalDeviceProfiles)
      } | sort > expected-external
      (cd "${inputplumberRoot}/devices"; printf '%s\n' 60-*.yaml 65-generic_gamepad.yaml) | sort > actual-external
      cmp expected-external actual-external || exit 1
      default_profile="${inputplumberRoot}/profiles/default.yaml"
      resolved_profile="${inputplumberRoot}/profiles/${resolvedProfile}"

      test -f "$default_profile" || {
        echo "selected upstream profile default.yaml is missing" >&2
        exit 1
      }

      ${lib.concatMapStringsSep "\n" (profile: ''
        selected_device="${inputplumberRoot}/devices/${profile.file}"
        test -f "$selected_device" || {
          echo "selected upstream profile ${profile.file} is missing" >&2
          exit 1
        }

        test "$(yq eval '.version' "$selected_device")" = 1 \
          && test "$(yq eval '.kind' "$selected_device")" = CompositeDevice \
          && test "$(yq eval '.name' "$selected_device")" = '${profile.name}' \
          && test "$(yq eval '.options.auto_manage // false' "$selected_device")" = false \
          && test "$(yq eval '.options.persist // false' "$selected_device")" = false \
          && test "$(yq eval -o=json -I=0 '.target_devices' "$selected_device")" = '${builtins.toJSON profile.upstreamTargets}' || {
            echo "selected upstream profile ${profile.file} changed schema or target policy" >&2
            exit 1
          }

        ${lib.optionalString (builtins.elem profile selectedDeviceProfiles) ''
          # Udev matchers are independent alternatives to typed matchers in
          # InputPlumber. Every alternative must constrain input/event nodes.
          test "$(yq eval '.maximum_sources' "$selected_device")" = 1 \
            && test "$(yq eval '.source_devices | length' "$selected_device")" -gt 0 \
            && test "$(yq eval '[.source_devices[] | select(
              has("hidraw") or has("iio") or has("led") or has("tty")
              or (.evdev == null and .udev == null)
              or (.evdev != null and .evdev.handler != "event*")
              or (.udev != null and (.udev.subsystem != "input" or .udev.sys_name != "event*"))
            )] | length' "$selected_device")" = 0 || {
            echo "selected profile ${profile.file} no longer guarantees one evdev source" >&2
            exit 1
          }
          yq eval -i '.options.auto_manage = true | .options.persist = true | .target_devices = ["xb360", "mouse", "keyboard"]' "$selected_device"
          test "$(yq eval '.options.auto_manage' "$selected_device")" = true \
            && test "$(yq eval '.options.persist' "$selected_device")" = true \
            && test "$(yq eval -o=json -I=0 '.target_devices' "$selected_device")" = '["xb360","mouse","keyboard"]' || {
            echo "selected profile ${profile.file} auto-management or target transform produced unexpected data" >&2
            exit 1
          }
        ''}
      '') externalDeviceProfiles}

      # An active ignore config before all 60-* and 65-* profiles uses the
      # exact upstream matchers, not a second hand-written vendor/model list.
      deferred_profile="${inputplumberRoot}/devices/59-korri-deferred-gamepads.yaml"
      test ! -e "$deferred_profile" || exit 1
      printf '%s\n' 'version: 1' 'kind: CompositeDevice' 'name: Deferred multi-interface gamepads' \
        'matches: []' 'options:' '  auto_manage: true' 'source_devices: []' > "$deferred_profile"
      ${lib.concatMapStringsSep "\n" (profile: ''
        UPSTREAM="${inputplumberRoot}/devices/${profile.file}" yq eval -i \
          '.source_devices += load(strenv(UPSTREAM)).source_devices | .source_devices[].ignore = true' "$deferred_profile"
      '') deferredDeviceProfiles}

      test "$(yq eval '.version' "$default_profile")" = 1 \
        && test "$(yq eval '.kind' "$default_profile")" = DeviceProfile \
        && test "$(yq eval '.name' "$default_profile")" = Default \
        && test "$(yq eval '.mapping | type' "$default_profile")" = '!!seq' \
        && test "$(yq eval '[.mapping[].target_events[] | select(has("dbus"))] | length' "$default_profile")" = 0 || {
          echo "selected upstream profile default.yaml changed schema or already contains Korri DBus routing" >&2
          exit 1
        }

      test ! -e "$resolved_profile" || {
        echo "Korri resolved profile ${resolvedProfile} already exists upstream" >&2
        exit 1
      }

      cp "$default_profile" "$resolved_profile"
      yq eval -i '.name = "Korri Microsoft X-Box One pad"' "$resolved_profile"
      cat ${shortcutMappings} >> "$resolved_profile"

      upstream_mapping_count="$(yq eval '.mapping | length' "$default_profile")"
      shortcut_mapping_count="$(yq eval 'length' ${shortcutMappings})"
      test "$shortcut_mapping_count" -gt 0 \
        && test "$(yq eval '.mapping | length' "$resolved_profile")" \
          -eq "$((upstream_mapping_count + shortcut_mapping_count))" \
        && test "$(yq eval -o=json -I=0 ".mapping | .[$upstream_mapping_count:]" "$resolved_profile")" \
          = "$(yq eval -o=json -I=0 '.' ${shortcutMappings})" || {
          echo "Korri resolved profile must append the authoritative shortcut mappings exactly once" >&2
          exit 1
        }
    '';

  composeResolved =
    {
      inputplumberKorri,
      additionalDataPackages ? [ ],
    }:
    pkgs.runCommand "inputplumber-korri-resolved-${inputplumberKorri.version}"
      {
        pname = "inputplumber-korri";
        version = inputplumberKorri.version;
        passthru = (inputplumberKorri.passthru or { }) // {
          inherit additionalDataPackages;
        };
        meta = inputplumberKorri.meta;
      }
      ''
        set -euo pipefail
        cp -a ${inputplumberKorri} "$out"
        chmod -R u+w "$out"
        inputplumber_root="$out/share/inputplumber"
        ${lib.concatMapStringsSep "\n" (dataPackage: ''
          additional_root="${dataPackage}/share/inputplumber"
          test -d "$additional_root" || {
            echo "additional InputPlumber data package ${dataPackage} has no share/inputplumber root" >&2
            exit 1
          }
          while IFS= read -r -d $'\0' additional_file; do
            relative_path="''${additional_file#"$additional_root/"}"
            destination="$inputplumber_root/$relative_path"
            if test -e "$destination" || test -L "$destination"; then
              echo "additional InputPlumber data collides with resolved path $relative_path" >&2
              exit 1
            fi
            mkdir -p "$(dirname "$destination")"
            cp -a "$additional_file" "$destination"
          done < <(find "$additional_root" \( -type f -o -type l \) -print0 | sort -z)
        '') additionalDataPackages}
      '';

  compose =
    {
      inputplumberRuntime,
      additionalDataPackages ? [ ],
    }:
    pkgs.runCommand "inputplumber-korri-${inputplumberRuntime.version}"
      {
        pname = "inputplumber-korri";
        version = inputplumberRuntime.version;
        nativeBuildInputs = [ pkgs.yq-go ];
        passthru = {
          upstream = inputplumberRuntime.upstream;
          inherit selectedDeviceProfiles resolvedProfile additionalDataPackages;
        };
        meta = inputplumberRuntime.meta // {
          description = "Pinned InputPlumber with Korri's resolved input data root";
          mainProgram = "inputplumber";
        };
      }
      ''
        set -euo pipefail

        cp -a ${inputplumberRuntime} "$out"
        chmod -R u+w "$out"
        inputplumber_root="$out/share/inputplumber"

        ${lib.concatMapStringsSep "\n" (dataPackage: ''
          additional_root="${dataPackage}/share/inputplumber"
          test -d "$additional_root" || {
            echo "additional InputPlumber data package ${dataPackage} has no share/inputplumber root" >&2
            exit 1
          }
          while IFS= read -r -d $'\0' additional_file; do
            relative_path="''${additional_file#"$additional_root/"}"
            destination="$inputplumber_root/$relative_path"
            if test -e "$destination" || test -L "$destination"; then
              echo "additional InputPlumber data collides with resolved path $relative_path" >&2
              exit 1
            fi
            mkdir -p "$(dirname "$destination")"
            cp -a "$additional_file" "$destination"
          done < <(find "$additional_root" \( -type f -o -type l \) -print0 | sort -z)
        '') additionalDataPackages}

        ${transformSelectedProfiles { inputplumberRoot = "$inputplumber_root"; }}
        test ! -e "$inputplumber_root/devices/00-korri-ignore-virtual.yaml"
        cp ${./inputplumber-ignore-virtual.yaml} "$inputplumber_root/devices/00-korri-ignore-virtual.yaml"
      '';
in
{
  inherit
    compose
    composeResolved
    transformSelectedProfiles
    selectedDeviceProfiles
    externalDeviceProfiles
    deferredDeviceProfiles
    resolvedProfile
    ;
}
