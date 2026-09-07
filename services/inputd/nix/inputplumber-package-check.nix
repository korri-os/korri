{
  pkgs,
  inputplumberRuntime,
  inputplumberKorri,
  retroarchInputplumberAutoconfig,
}:

let
  data = import ./inputplumber-data.nix { inherit pkgs; };
  additionalData = pkgs.runCommand "inputplumber-additional-data-check-fixture" { } ''
    mkdir -p "$out/share/inputplumber/devices"
    # This is the real platform-owned profile path selected by legacy's SM8550
    # composition. The content remains owned by the platform data package.
    touch "$out/share/inputplumber/devices/02-ayn-controller.yaml"
  '';
  inputplumberWithAdditionalData = data.compose {
    inherit inputplumberRuntime;
    additionalDataPackages = [ additionalData ];
  };
in
assert inputplumberRuntime.version == "0.75.2";
assert inputplumberRuntime.upstream.owner == "ShadowBlip";
assert inputplumberRuntime.upstream.repo == "InputPlumber";
assert inputplumberRuntime.upstream.tag == "v0.75.2";
assert
  inputplumberRuntime.upstream.sourceHash == "sha256-KiSroDcaWvzr5sP0jzr1GFyk0lHbtCFJrP3g5/b3hLQ=";
assert
  inputplumberRuntime.upstream.cargoHash == "sha256-VwQ38Jv5OvyBqo9BBTnpUjgNwAbWyIdUKFKXsGC6+Mo=";
assert inputplumberRuntime.upstream.patches == [ ./inputplumber-dbus-observer-timing.patch ];
assert inputplumberRuntime.patches == inputplumberRuntime.upstream.patches;
assert inputplumberRuntime.doCheck;
assert inputplumberKorri.version == inputplumberRuntime.version;
assert inputplumberKorri.upstream == inputplumberRuntime.upstream;
pkgs.runCommand "inputplumber-korri-package-check"
  {
    nativeBuildInputs = [ pkgs.yq-go ];
  }
  ''
    set -euo pipefail

    route_source_count=0
    route_source=""
    for candidate in ${./.}/*; do
      if test -f "$candidate" && grep -Eq 'ui_[[:alpha:]]' "$candidate"; then
        route_source_count=$((route_source_count + 1))
        route_source="$(basename "$candidate")"
      fi
    done
    test "$route_source_count" = 1 \
      && test "$route_source" = inputplumber-korri-dbus-shortcuts.yaml || {
        echo "shortcut routes must have one authoritative mapping source" >&2
        exit 1
      }

    inputplumber_bin="${inputplumberKorri}/bin/inputplumber"
    inputplumber_root="${inputplumberKorri}/share/inputplumber"
    inputplumber_manager="${inputplumberRuntime.upstream.src}/src/input/manager.rs"
    selected_device="$inputplumber_root/devices/${data.selectedDeviceProfile}"
    default_profile="$inputplumber_root/profiles/default.yaml"
    resolved_profile="$inputplumber_root/profiles/${data.resolvedProfile}"

    test -x "$inputplumber_bin" || {
      echo "inputplumber-korri must expose bin/inputplumber" >&2
      exit 1
    }
    head -c 4 "$inputplumber_bin" | grep -q $'\177ELF' || {
      echo "inputplumber-korri bin/inputplumber must be a native executable" >&2
      exit 1
    }
    "$inputplumber_bin" --version | grep -F '0.75.2' >/dev/null || {
      echo "inputplumber-korri must report upstream version 0.75.2" >&2
      exit 1
    }
    test "$(grep -Fc 'let dbus_device = self.create_target_device("dbus").await?;' "$inputplumber_manager")" = 1 || {
      echo "pinned InputPlumber must create one automatic DBus target per composite" >&2
      exit 1
    }

    test -f "$selected_device" || {
      echo "selected upstream profile ${data.selectedDeviceProfile} is missing" >&2
      exit 1
    }
    test "$(yq eval '.options.auto_manage' "$selected_device")" = true || {
      echo "selected device profile must auto-manage its supported physical controllers" >&2
      exit 1
    }
    test "$(yq eval '.options.persist' "$selected_device")" = true || {
      echo "selected device profile must preserve its normalized target across source reconnects" >&2
      exit 1
    }
    test "$(yq eval '[.source_devices[].evdev | select(.vendor_id == "045e" and (.product_id | contains("02ea")))] | length' "$selected_device")" = 1 || {
      echo "selected device profile must match Sunshine's exact virtual Xbox One identity" >&2
      exit 1
    }
    test "$(yq eval '[.target_devices[] | select(. == "xb360")] | length' "$selected_device")" = 1 || {
      echo "selected device profile must target xb360 exactly once" >&2
      exit 1
    }
    test "$(yq eval '[.target_devices[] | select(. == "dbus")] | length' "$selected_device")" = 0 || {
      echo "selected device profile must use only InputPlumber's one automatic composite DBus target" >&2
      exit 1
    }
    test "$(yq eval '[.target_devices[] | select(. == "xbox-series")] | length' "$selected_device")" = 0 || {
      echo "selected device profile still contains superseded xbox-series target" >&2
      exit 1
    }

    cmp "${inputplumberRuntime}/share/inputplumber/profiles/default.yaml" "$default_profile" || {
      echo "the shared upstream default profile must remain unchanged" >&2
      exit 1
    }
    test -f "$resolved_profile" || {
      echo "the selected device must have a Korri-specific resolved profile" >&2
      exit 1
    }
    for profile in "$inputplumber_root"/profiles/*.yaml; do
      if test "$profile" = "$resolved_profile"; then
        continue
      fi
      profile_name="$(basename "$profile")"
      cmp "${inputplumberRuntime}/share/inputplumber/profiles/$profile_name" "$profile" || {
        echo "unselected profile $profile_name must remain unchanged" >&2
        exit 1
      }
      if grep -q '^  - name: Korri ' "$profile"; then
        echo "unselected profile $profile_name contains Korri routes" >&2
        exit 1
      fi
    done

    upstream_mapping_count="$(yq eval '.mapping | length' "$default_profile")"
    shortcut_mapping_count="$(yq eval 'length' ${./inputplumber-korri-dbus-shortcuts.yaml})"
    test "$(yq eval '.mapping | length' "$resolved_profile")" \
      -eq "$((upstream_mapping_count + shortcut_mapping_count))" \
      && test "$(yq eval -o=json -I=0 ".mapping | .[$upstream_mapping_count:]" "$resolved_profile")" \
        = "$(yq eval -o=json -I=0 '.' ${./inputplumber-korri-dbus-shortcuts.yaml})" || {
        echo "resolved profile must contain the authoritative shortcut mappings exactly once" >&2
        exit 1
      }

    test "$(yq eval '[.[] | select(.source_event.gamepad.button == "Guide")] | length' ${./inputplumber-korri-dbus-shortcuts.yaml})" = 1 \
      && test "$(yq eval '[.[] | select(.source_event.gamepad.button == "Guide") | .target_events[] | select(has("dbus"))] | length' ${./inputplumber-korri-dbus-shortcuts.yaml})" = 1 \
      && test "$(yq eval '[.[] | select(.source_event.gamepad.button == "Guide") | .target_events[] | select(has("gamepad"))] | length' ${./inputplumber-korri-dbus-shortcuts.yaml})" = 0 || {
        echo "Guide must have one DBus-only authoritative route" >&2
        exit 1
      }

    while IFS= read -r button; do
      test "$(BUTTON="$button" yq eval '[.[] | select(.source_event.gamepad.button == env(BUTTON)) | .target_events[] | select(.gamepad.button == env(BUTTON))] | length' ${./inputplumber-korri-dbus-shortcuts.yaml})" = 1 \
        && test "$(BUTTON="$button" yq eval '[.[] | select(.source_event.gamepad.button == env(BUTTON)) | .target_events[] | select(has("dbus"))] | length' ${./inputplumber-korri-dbus-shortcuts.yaml})" = 1 || {
          echo "$button must retain gameplay and have one DBus route" >&2
          exit 1
        }
    done < <(yq eval -r '.[] | select(.source_event.gamepad.button != "Guide") | .source_event.gamepad.button' ${./inputplumber-korri-dbus-shortcuts.yaml})

    retroarch_autoconfig_root="${retroarchInputplumberAutoconfig}/share/libretro/autoconfig"
    retroarch_xbox_config="$retroarch_autoconfig_root/udev/Microsoft X-Box 360 pad.cfg"
    upstream_xbox_config="${pkgs.retroarch-joypad-autoconfig}/share/libretro/autoconfig/udev/Microsoft X-Box 360 pad.cfg"
    test -f "$retroarch_xbox_config" || {
      echo "Korri's Xbox 360 RetroArch autoconfig is missing" >&2
      exit 1
    }
    sed '/^input_menu_toggle_btn\(_label\)\? =/d' "$upstream_xbox_config" \
      | cmp - "$retroarch_xbox_config" || {
        echo "ordinary Xbox 360 gameplay autoconfig bindings changed" >&2
        exit 1
      }
    test "$(grep -Ec '^input_(b|y|select|start|up|down|left|right|a|x|l|r|l2|r2|l3|r3)_(btn|axis) =' "$retroarch_xbox_config")" = 16 || {
      echo "Xbox 360 gameplay bindings are incomplete" >&2
      exit 1
    }
    if grep -Eq '^input_menu_toggle_btn(_label)? =' "$retroarch_xbox_config"; then
      echo "Guide/Home must not be captured by RetroArch autoconfig" >&2
      exit 1
    fi

    autoconfig_drift_root="$TMPDIR/autoconfig-drift"
    mkdir -p "$autoconfig_drift_root/udev"
    cp "$upstream_xbox_config" "$autoconfig_drift_root/udev/Microsoft X-Box 360 pad.cfg"
    chmod u+w "$autoconfig_drift_root/udev/Microsoft X-Box 360 pad.cfg"
    printf '\ninput_unreviewed_btn = "99"\n' \
      >> "$autoconfig_drift_root/udev/Microsoft X-Box 360 pad.cfg"
    if (
      ${retroarchInputplumberAutoconfig.transform {
        upstreamRoot = "$autoconfig_drift_root";
        outputRoot = "$TMPDIR/autoconfig-drift-output";
      }}
    ); then
      echo "changed upstream Xbox 360 autoconfig unexpectedly passed composition" >&2
      exit 1
    fi
    rm "$autoconfig_drift_root/udev/Microsoft X-Box 360 pad.cfg"
    if (
      ${retroarchInputplumberAutoconfig.transform {
        upstreamRoot = "$autoconfig_drift_root";
        outputRoot = "$TMPDIR/autoconfig-missing-output";
      }}
    ); then
      echo "missing upstream Xbox 360 autoconfig unexpectedly passed composition" >&2
      exit 1
    fi

    additional_root="${inputplumberWithAdditionalData}/share/inputplumber"
    test -f "$additional_root/devices/02-ayn-controller.yaml" || {
      echo "additional platform data was not composed into the resolved root" >&2
      exit 1
    }
    test -f "$additional_root/schema/device_profile_v1.json" || {
      echo "upstream runtime data was lost while composing additional data" >&2
      exit 1
    }
    test -x "${inputplumberWithAdditionalData}/bin/inputplumber" || {
      echo "upstream runtime executable was lost while composing additional data" >&2
      exit 1
    }

    drift_root="$TMPDIR/schema-drift/share/inputplumber"
    mkdir -p "$drift_root/devices" "$drift_root/profiles"
    cp "${inputplumberRuntime}/share/inputplumber/devices/${data.selectedDeviceProfile}" \
      "$drift_root/devices/${data.selectedDeviceProfile}"
    cp "${inputplumberRuntime}/share/inputplumber/profiles/default.yaml" \
      "$drift_root/profiles/default.yaml"
    yq eval -i '.target_devices[0] = "renamed-upstream-target"' \
      "$drift_root/devices/${data.selectedDeviceProfile}"
    if (
      ${data.transformSelectedProfile { inputplumberRoot = "$drift_root"; }}
    ); then
      echo "changed selected-profile schema unexpectedly passed composition" >&2
      exit 1
    fi

    rm "$drift_root/devices/${data.selectedDeviceProfile}"
    if (
      ${data.transformSelectedProfile { inputplumberRoot = "$drift_root"; }}
    ); then
      echo "missing selected profile unexpectedly passed composition" >&2
      exit 1
    fi

    touch "$out"
  ''
