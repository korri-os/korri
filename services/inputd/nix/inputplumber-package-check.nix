{
  pkgs,
  inputplumberRuntime,
  inputplumberKorri,
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
assert
  inputplumberRuntime.upstream.patches == [
    ./inputplumber-dbus-observer-timing.patch
    ./inputplumber-target-device-paths.patch
    ./inputplumber-startup-unhide-quiet.patch
    ./inputplumber-ignore-before-reconnect.patch
  ];
assert inputplumberRuntime.patches == inputplumberRuntime.upstream.patches;
assert inputplumberRuntime.doCheck;
assert inputplumberKorri.version == inputplumberRuntime.version;
assert inputplumberKorri.upstream == inputplumberRuntime.upstream;
pkgs.runCommand "inputplumber-korri-package-check"
  {
    nativeBuildInputs = [
      pkgs.yq-go
      pkgs.libxml2
    ];
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

    policy="${inputplumberRuntime}/share/polkit-1/actions/org.shadowblip.InputPlumber.policy"
    xmllint --nonet --noout "$policy"
    action='/policyconfig/action[@id="org.shadowblip.Input.Target.DevicePaths"]'
    test "$(xmllint --xpath "count($action)" "$policy")" = 1
    test "$(xmllint --xpath "count($action/defaults/*[text()='no'])" "$policy")" = 3

    # Regression for #437: every external profile must adopt its controllers
    # without ManageAllDevices, including the unknown-joystick catch-all.
    for selected in ${
      pkgs.lib.escapeShellArgs (map (profile: profile.file) data.selectedDeviceProfiles)
    }; do
      selected_device="$inputplumber_root/devices/$selected"
      test -f "$selected_device"
      test "$(yq eval '.version' "$selected_device")" = 1 \
        && test "$(yq eval '.kind' "$selected_device")" = CompositeDevice
      test "$(yq eval '.options.auto_manage' "$selected_device")" = true || {
        echo "$selected must auto-manage its supported physical controllers" >&2
        exit 1
      }
      test "$(yq eval '.options.persist' "$selected_device")" = true || {
        echo "$selected must preserve its normalized target across source reconnects" >&2
        exit 1
      }
      test "$(yq eval -o=json -I=0 '.target_devices' "$selected_device")" = '["xb360","mouse","keyboard"]' || {
        echo "$selected must target xb360, mouse and keyboard with only the automatic DBus target" >&2
        exit 1
      }
      # Device matching must remain upstream-owned, including third-party pads.
      cmp <(yq eval -o=json '.source_devices' "$selected_device") \
        <(yq eval -o=json '.source_devices' "${inputplumberRuntime}/share/inputplumber/devices/$selected")
    done
    test "$(yq eval '[.source_devices[].evdev | select(.vendor_id == "045e" and (.product_id | contains("02ea")))] | length' "$inputplumber_root/devices/60-xbox_one_gamepad.yaml")" = 1 || {
      echo "Xbox One profile must match Sunshine's exact virtual Xbox One identity" >&2
      exit 1
    }
    test "$(yq eval '[.source_devices[].evdev | select(.vendor_id == "045e" and (.product_id | contains("028e")))] | length' "$inputplumber_root/devices/60-xbox_360_gamepad.yaml")" = 1 || {
      echo "Xbox 360 profile must match the wired Microsoft 045e:028e pad" >&2
      exit 1
    }
    for device in "$inputplumber_root"/devices/*.yaml; do
      case "$(basename "$device")" in
        ${
          pkgs.lib.concatMapStringsSep "|" (profile: profile.file) data.selectedDeviceProfiles
        }|00-korri-ignore-virtual.yaml|59-korri-deferred-gamepads.yaml) continue ;;
      esac
      cmp "${inputplumberRuntime}/share/inputplumber/devices/$(basename "$device")" "$device" || {
        echo "unselected device profile $device must remain unchanged" >&2
        exit 1
      }
    done

    cmp ${./inputplumber-ignore-virtual.yaml} "$inputplumber_root/devices/00-korri-ignore-virtual.yaml"
    deferred="$inputplumber_root/devices/59-korri-deferred-gamepads.yaml"
    test "$(yq eval '.options.auto_manage' "$deferred")" = true
    test "$(yq eval '[.source_devices[] | select(.ignore != true)] | length' "$deferred")" = 0
    : > expected-deferred
    ${pkgs.lib.concatMapStringsSep "\n" (profile: ''
      yq eval -o=json -I=0 '.source_devices[] | del(.ignore)' \
        "${inputplumberRuntime}/share/inputplumber/devices/${profile.file}" >> expected-deferred
    '') data.deferredDeviceProfiles}
    yq eval -o=json -I=0 '.source_devices[] | del(.ignore)' "$deferred" > actual-deferred
    cmp expected-deferred actual-deferred
    # The upstream unsupported rule excludes js/mouse handlers, not event
    # joysticks. The broad event ignore is later than our generic catch-all.
    test "$(yq eval '[.source_devices[].evdev.handler | select(. == "event*")] | length' "$inputplumber_root/devices/10-ignore_unsupported.yaml")" = 0
    test "$(yq eval '[.source_devices[].evdev.handler | select(. == "event*")] | length' "$inputplumber_root/devices/69-ignore_generic.yaml")" = 1
    printf '%s\n' 00-korri-ignore-virtual.yaml 10-ignore_unsupported.yaml \
      59-korri-deferred-gamepads.yaml 60-xbox_360_gamepad.yaml \
      65-generic_gamepad.yaml 69-ignore_generic.yaml > expected-order
    sort expected-order > actual-order
    cmp expected-order actual-order

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

    prepare_drift_root() {
      mkdir -p "$drift_root/devices" "$drift_root/profiles"
      cp "${inputplumberRuntime}/share/inputplumber/devices/"*.yaml "$drift_root/devices/"
      cp "${inputplumberRuntime}/share/inputplumber/profiles/default.yaml" \
        "$drift_root/profiles/default.yaml"
      # A valid transform must be able to append mappings to its copied
      # profile. Otherwise read-only store modes masquerade as guard refusals.
      chmod -R u+w "$drift_root"
    }
    # Positive control: a refusal must come from a mutation, not the harness.
    drift_root="$TMPDIR/schema-drift/control/share/inputplumber"
    prepare_drift_root
    (
      ${data.transformSelectedProfiles { inputplumberRoot = "$drift_root"; }}
    )

    for selected in ${
      pkgs.lib.escapeShellArgs (map (profile: profile.file) data.externalDeviceProfiles)
    }; do
      source_mutations=()
      case "$selected" in
        ${pkgs.lib.concatMapStringsSep "|" (profile: profile.file) data.selectedDeviceProfiles})
          source_mutations=(
            '.source_devices[0].udev.subsystem = "hidraw"'
            '.maximum_sources = 2'
            '.source_devices[0].hidraw = {"vendor_id": "045e"}'
            '.source_devices = []'
            '.source_devices[0].evdev.handler = "js*"'
            '.source_devices[0].udev.sys_name = "js*"'
          ) ;;
      esac
      for mutation in "''${source_mutations[@]}" '.version = 2' '.kind = "DeviceProfile"' '.name = "renamed"' \
        '.options.auto_manage = true' '.options.persist = true' \
        '.target_devices[0] = "renamed-upstream-target"' missing; do
        drift_root="$TMPDIR/schema-drift/$selected/$mutation/share/inputplumber"
        prepare_drift_root
        if test "$mutation" = missing; then
          rm "$drift_root/devices/$selected"
        else
          yq eval -i "$mutation" "$drift_root/devices/$selected"
        fi
        if (
          ${data.transformSelectedProfiles { inputplumberRoot = "$drift_root"; }}
        ); then
          echo "$selected with $mutation unexpectedly passed composition" >&2
          exit 1
        fi
      done
    done

    touch "$out"
  ''
