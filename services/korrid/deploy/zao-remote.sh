#!/run/current-system/sw/bin/bash
set -euo pipefail

# Stop the only process that can prepare a game before the one-off cut from
# the obsolete launch-id atom. The game unit proof then closes the race: no
# new prepare can start between the proof and the removal.
quiesce_and_cut_obsolete_session() {
  local session_root="$HOME/.local/state/korrid/private/host-session"
  local active_units entries atom mode
  systemctl --user stop korrid.service >/dev/null 2>&1 || true
  ! systemctl --user is-active --quiet korrid.service \
    || { echo "korrid launch authority remained active" >&2; return 1; }
  active_units="$(systemctl --user list-units --type=service \
    --state=activating,active,reloading,deactivating --no-legend --plain \
    'korri-game-*.service' 2>/dev/null)" \
    || { echo "Korri game unit state is unavailable" >&2; return 1; }
  [[ -z "$active_units" ]] \
    || { echo "a Korri game unit is live" >&2; return 1; }
  [[ ! -L "$session_root" ]] || return 1
  [[ -e "$session_root" ]] || return 0
  [[ -d "$session_root" ]] || return 1
  mode="$(stat -Lc '%a' -- "$session_root" 2>/dev/null)" || return 1
  [[ "$mode" == 700 ]] || return 1
  entries="$(find "$session_root" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null)" \
    || return 1
  [[ -n "$entries" ]] || return 0
  [[ "$entries" == launch-id ]] || return 1
  atom="$session_root/launch-id"
  [[ ! -L "$atom" && -f "$atom" ]] || return 1
  [[ "$(stat -Lc '%a' -- "$atom" 2>/dev/null)" == 600 ]] || return 1
  [[ "$(cat -- "$atom" 2>/dev/null)" =~ ^[0-9a-f]{32}$ ]] || return 1
  rm -- "$atom"
  sync -f "$session_root"
}

action="${1:-}"
case "$action" in
  install)
    package="${2:?package store path required}"
    handoff="${3:?private handoff directory required}"
    revision="$(< "$handoff/revision")"
    if [[ ! -x "$HOME/.nix-profile/bin/neverball" ]]; then
      echo "Neverball is not provisioned; run the provision-game action first" >&2
      exit 1
    fi
    if [[ ! -f "$HOME/.local/share/korri/roms/wl4.gba" ]]; then
      echo "Wario Land 4 is not provisioned outside the Nix store" >&2
      exit 1
    fi
    mkdir -p \
      "$HOME/.config/korrid" \
      "$HOME/.config/systemd/user" \
      "$HOME/.local/libexec" \
      "$HOME/.local/share/korri" \
      "$HOME/.local/state/korrid/profiles"
    install -d -m 0700 "$HOME/.local/state/korrid/private"
    deployed_documents="$HOME/.local/state/korrid/deployed-documents.sha256"
    candidate_documents="$(cat "$handoff/config.yaml" "$handoff/library.yaml" | sha256sum | cut -d' ' -f1)"
    current_config="$HOME/.local/share/korri/config.yaml"
    current_library="$HOME/.local/share/korri/library.yaml"
    if [[ -f "$current_config" || -f "$current_library" ]]; then
      if [[ ! -f "$current_config" || ! -f "$current_library" ]]; then
        echo "refusing to overwrite a partial external Korri configuration" >&2
        exit 1
      fi
      current_documents="$(cat "$current_config" "$current_library" | sha256sum | cut -d' ' -f1)"
      if [[ -f "$deployed_documents" ]]; then
        if [[ "$current_documents" != "$(< "$deployed_documents")" ]]; then
          echo "refusing to overwrite externally edited Korri configuration" >&2
          exit 1
        fi
      elif [[ "$current_documents" != "$candidate_documents" ]]; then
        echo "refusing to replace untracked Korri configuration" >&2
        exit 1
      fi
    fi
    profile="$HOME/.local/state/korrid/profiles/$(basename "$package")"
    if [[ ! -e "$profile" ]]; then
      nix profile install --profile "$profile" "$package"
    fi

    previous_current=""
    previous_service_active=false
    if [[ -L "$HOME/.local/state/korrid/current" ]]; then
      previous_current="$(readlink "$HOME/.local/state/korrid/current")"
    fi
    systemctl --user is-active --quiet korrid.service && previous_service_active=true
    previous_config="$handoff/host.toml.previous"
    had_previous_config=false
    if [[ -f "$HOME/.config/korrid/host.toml" ]]; then
      cp "$HOME/.config/korrid/host.toml" "$previous_config"
      had_previous_config=true
    fi
    previous_device_config="$handoff/config.yaml.previous"
    previous_library="$handoff/library.yaml.previous"
    had_previous_device_config=false
    had_previous_library=false
    if [[ -f "$HOME/.local/share/korri/config.yaml" ]]; then
      cp "$HOME/.local/share/korri/config.yaml" "$previous_device_config"
      had_previous_device_config=true
    fi
    if [[ -f "$HOME/.local/share/korri/library.yaml" ]]; then
      cp "$HOME/.local/share/korri/library.yaml" "$previous_library"
      had_previous_library=true
    fi
    previous_unit="$handoff/korrid.service.previous"
    had_previous_unit=false
    if [[ -f "$HOME/.config/systemd/user/korrid.service" ]]; then
      cp "$HOME/.config/systemd/user/korrid.service" "$previous_unit"
      had_previous_unit=true
    fi
    previous_environment="$handoff/environment.previous"
    had_previous_environment=false
    if [[ -f "$HOME/.config/korrid/environment" ]]; then
      cp "$HOME/.config/korrid/environment" "$previous_environment"
      had_previous_environment=true
    fi
    rollback_install() {
      trap - ERR
      if [[ -n "$previous_current" ]]; then
        ln -sfn "$previous_current" "$HOME/.local/state/korrid/current.next"
        mv -Tf \
          "$HOME/.local/state/korrid/current.next" \
          "$HOME/.local/state/korrid/current"
      else
        rm -f "$HOME/.local/state/korrid/current"
      fi
      if [[ "$had_previous_config" == true ]]; then
        install -m 0644 "$previous_config" "$HOME/.config/korrid/host.toml"
      else
        rm -f "$HOME/.config/korrid/host.toml"
      fi
      if [[ "$had_previous_device_config" == true ]]; then
        install -m 0644 "$previous_device_config" "$HOME/.local/share/korri/config.yaml"
      else
        rm -f "$HOME/.local/share/korri/config.yaml"
      fi
      if [[ "$had_previous_library" == true ]]; then
        install -m 0644 "$previous_library" "$HOME/.local/share/korri/library.yaml"
      else
        rm -f "$HOME/.local/share/korri/library.yaml"
      fi
      if [[ "$had_previous_unit" == true ]]; then
        install -m 0644 "$previous_unit" "$HOME/.config/systemd/user/korrid.service"
      else
        rm -f "$HOME/.config/systemd/user/korrid.service"
      fi
      if [[ "$had_previous_environment" == true ]]; then
        install -m 0600 "$previous_environment" "$HOME/.config/korrid/environment"
      else
        rm -f "$HOME/.config/korrid/environment"
      fi
      systemctl --user daemon-reload
      if [[ "$previous_service_active" == true ]]; then
        systemctl --user restart korrid.service || true
      else
        systemctl --user stop korrid.service || true
      fi
      echo "korrid deployment rolled back after failed health check" >&2
    }
    trap rollback_install ERR

    quiesce_and_cut_obsolete_session

    ln -sfn "$profile" "$HOME/.local/state/korrid/current.next"
    mv -Tf \
      "$HOME/.local/state/korrid/current.next" \
      "$HOME/.local/state/korrid/current"
    install -m 0644 "$handoff/korrid.service" \
      "$HOME/.config/systemd/user/korrid.service"
    install -m 0644 "$handoff/host.toml" \
      "$HOME/.config/korrid/host.toml"
    install -m 0600 "$handoff/environment" \
      "$HOME/.config/korrid/environment"
    install -m 0644 "$handoff/config.yaml" \
      "$HOME/.local/share/korri/config.yaml"
    install -m 0644 "$handoff/library.yaml" \
      "$HOME/.local/share/korri/library.yaml"
    install -m 0755 "$handoff/zao-remote.sh" \
      "$HOME/.local/libexec/korrid-deploy"
    systemctl --user daemon-reload
    systemctl --user enable korrid.service
    systemctl --user restart korrid.service

    healthy=false
    for _ in $(seq 1 40); do
      if response="$(curl --fail --silent --connect-timeout 1 --max-time 2 \
        http://127.0.0.1:43117/rpc \
        -H 'content-type: application/json' \
        -d '{"_tag":"app.catalog.snapshot","payload":{}}')" && \
        [[ "$response" == *'"id":"neverball"'* ]] && \
        [[ "$response" == *'"id":"wl4"'* ]] && \
        [[ "$response" == *'"title":"Wario Land 4"'* ]] && \
        [[ "$response" == *'"host":"zao"'* ]] && \
        [[ "$response" == *'"kind":"hash","value":"sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414"'* ]]; then
        healthy=true
        break
      fi
      sleep 0.25
    done
    if [[ "$healthy" != true ]]; then
      echo "candidate korrid did not serve the expected Neverball and Wario Land 4 catalog" >&2
      false
    fi

    printf '%s\n' "$candidate_documents" > "$deployed_documents.next"
    mv -f "$deployed_documents.next" "$deployed_documents"
    printf '%s\n' "$revision" > "$HOME/.local/state/korrid/deployed-revision"
    trap - ERR
    ;;
  provision-game)
    nix profile install nixpkgs#neverball
    ;;
  restart)
    systemctl --user restart korrid.service
    ;;
  logs)
    exec journalctl --user -u korrid.service -n 100 --no-pager
    ;;
  *)
    echo "usage: $0 {install <store-path> <handoff-dir>|provision-game|restart|logs}" >&2
    exit 2
    ;;
esac
