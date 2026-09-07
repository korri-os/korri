#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils curl findutils nix systemd
# shellcheck shell=bash
set -Eeuo pipefail

# This installer owns only the user service. Refuse a system launch authority
# instead of stopping it. Prove both managers idle before the one-off cut; a
# failed cut restores only prior user-service activity, not game or install state.
quiesce_and_cut_obsolete_session() {
  (
    local session_root="$HOME/.local/state/korrid/private/host-session"
    local active_units entries atom mode manager unit state previous_user_state
    # The system manager hosts the shipped game backend and activation socket.
    # A failed query is not an inactivity proof.
    for unit in korrid.service korrid-control.socket; do
      state="$(systemctl --system show "$unit" -p ActiveState --value)" || return 1
      case "$state" in
        inactive|failed) ;;
        *) echo "system launch authority is not inactive: $unit ($state)" >&2; return 1 ;;
      esac
    done
    previous_user_state="$(systemctl --user show korrid.service -p ActiveState --value)" || return 1
    case "$previous_user_state" in
      active|inactive|failed) ;;
      *) echo "user launch authority activity is unsettled" >&2; return 1 ;;
    esac
    trap 'if [[ "$previous_user_state" == active ]]; then systemctl --user start korrid.service; else systemctl --user stop korrid.service; fi' EXIT
    systemctl --user stop korrid.service >/dev/null 2>&1 || true
    state="$(systemctl --user show korrid.service -p ActiveState --value)" || return 1
    case "$state" in
      inactive|failed) ;;
      *) echo "korrid launch authority remained active" >&2; return 1 ;;
    esac
    for manager in --system --user; do
      active_units="$(systemctl "$manager" list-units --type=service \
        --state=activating,active,reloading,deactivating --no-legend --plain \
        'korri-game-*.service' 2>/dev/null)" \
        || { echo "$manager Korri game unit state is unavailable" >&2; return 1; }
      [[ -z "$active_units" ]] \
        || { echo "a $manager Korri game unit is live" >&2; return 1; }
    done
    [[ ! -L "$session_root" ]] || return 1
    [[ -e "$session_root" ]] || { trap - EXIT; return 0; }
    [[ -d "$session_root" ]] || return 1
    mode="$(stat -Lc '%a' -- "$session_root" 2>/dev/null)" || return 1
    [[ "$mode" == 700 ]] || return 1
    entries="$(find "$session_root" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null)" \
      || return 1
    [[ -n "$entries" ]] || { trap - EXIT; return 0; }
    [[ "$entries" == launch-id ]] || return 1
    atom="$session_root/launch-id"
    [[ ! -L "$atom" && -f "$atom" ]] || return 1
    [[ "$(stat -Lc '%a' -- "$atom" 2>/dev/null)" == 600 ]] || return 1
    [[ "$(cat -- "$atom" 2>/dev/null)" =~ ^[0-9a-f]{32}$ ]] || return 1
    rm -- "$atom" || return 1
    sync -f "$session_root" || return 1
    trap - EXIT
  )
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
    storage_root="$HOME/.local/share/korri"
    # Validate the parent too: leaf checks cannot detect a blocking file or
    # prevent installation through a catalog symlink.
    if [[ -L "$storage_root/catalog" ]] ||
      [[ -e "$storage_root/catalog" && ! -d "$storage_root/catalog" ]]; then
      echo "unsafe external Korri catalog directory" >&2
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
    document_names=(device.yaml catalog/games.yaml catalog/releases.yaml)
    # Hash each document separately before combining; boundaries affect the digest.
    document_digest() (
      cd "$1"
      sha256sum "${document_names[@]}" | sha256sum | cut -d' ' -f1
    )
    for name in "${document_names[@]}"; do
      [[ -f "$handoff/$name" && ! -L "$handoff/$name" ]] || {
        echo "missing or unsafe candidate document: $name" >&2; exit 1;
      }
    done
    candidate_documents="$(document_digest "$handoff")"
    present_documents=0
    for name in "${document_names[@]}"; do
      if [[ -e "$storage_root/$name" || -L "$storage_root/$name" ]]; then
        [[ -f "$storage_root/$name" && ! -L "$storage_root/$name" ]] || {
          echo "unsafe external Korri document: $name" >&2; exit 1;
        }
        present_documents=$((present_documents + 1))
      fi
    done
    if [[ "$present_documents" -gt 0 ]]; then
      if [[ "$present_documents" -ne 3 ]]; then
        echo "refusing to overwrite a partial external Korri configuration" >&2
        exit 1
      fi
      current_documents="$(document_digest "$storage_root")"
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
    previous_documents="$handoff/documents.previous"
    mkdir -p "$previous_documents/catalog"
    for name in "${document_names[@]}"; do
      if [[ -f "$storage_root/$name" ]]; then
        cp "$storage_root/$name" "$previous_documents/$name"
        cmp -s "$storage_root/$name" "$previous_documents/$name"
      fi
    done
    catalog_was_present=false
    [[ ! -d "$storage_root/catalog" ]] || catalog_was_present=true
    had_previous_digest=false
    if [[ -f "$deployed_documents" ]]; then
      cp "$deployed_documents" "$handoff/documents.sha256.previous"
      had_previous_digest=true
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
      for name in "${document_names[@]}"; do
        if [[ -f "$previous_documents/$name" ]]; then
          install -D -m 0644 "$previous_documents/$name" "$storage_root/$name"
          cmp -s "$previous_documents/$name" "$storage_root/$name"
        else
          rm -f "$storage_root/$name"
        fi
      done
      if [[ "$catalog_was_present" != true ]]; then
        rmdir "$storage_root/catalog" 2>/dev/null || test ! -e "$storage_root/catalog"
      fi
      if [[ "$had_previous_digest" == true ]]; then
        cp "$handoff/documents.sha256.previous" "$deployed_documents"
      else
        rm -f "$deployed_documents"
      fi
      rm -f "$deployed_documents.next"
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

    if ! quiesce_and_cut_obsolete_session; then
      # No installation has changed yet. The cut restored prior activity;
      # generation rollback would unnecessarily rewrite files and restart korrid.
      trap - ERR
      exit 1
    fi

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
    mkdir -p "$storage_root/catalog"
    for name in "${document_names[@]}"; do
      install -m 0644 "$handoff/$name" "$storage_root/$name"
    done
    [[ "$(document_digest "$storage_root")" == "$candidate_documents" ]]
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
        [[ "$response" == *'"id":"01K4J6K8Y00000000000000002"'* ]] && \
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
