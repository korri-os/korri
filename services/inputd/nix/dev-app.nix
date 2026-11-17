{
  pkgs,
  inputdPackage,
  korridPackage,
}:

pkgs.writeShellApplication {
  name = "korri-dev";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.jq
    pkgs.systemd
  ];
  text = ''
    physical_input=disabled
    case "''${1:-}" in
      "") ;;
      --physical)
        physical_input=physical
        shift
        ;;
      *)
        echo "usage: korri-dev [--physical]" >&2
        exit 64
        ;;
    esac
    [[ "$#" -eq 0 ]] || {
      echo "usage: korri-dev [--physical]" >&2
      exit 64
    }

    runtime_parent="''${XDG_RUNTIME_DIR:-''${TMPDIR:-/tmp}}"
    runtime_root="$(mktemp -d "$runtime_parent/korri-dev.XXXXXXXX")"
    state_root="$runtime_root/state"
    signer_socket="$runtime_root/local-signer.sock"
    signer_credential_root="$runtime_root/signer-credentials"
    storage_root="$runtime_root/storage"
    mkdir -m 0700 "$state_root" "$signer_credential_root"
    mkdir -p "$storage_root"
    cat >"$runtime_root/host.toml" <<'EOF'
    label = "development"
    EOF

    children=()
    # shellcheck disable=SC2329 # Invoked indirectly by the traps below.
    cleanup() {
      local pid
      trap - EXIT INT TERM
      for pid in "''${children[@]}"; do
        kill -TERM "$pid" 2>/dev/null || true
      done
      for pid in "''${children[@]}"; do
        wait "$pid" 2>/dev/null || true
      done
      rm -rf "$runtime_root"
    }
    trap cleanup EXIT INT TERM

    echo "Korri development runtime: $runtime_root"
    echo "Input source: $physical_input"
    if [[ "$physical_input" == disabled ]]; then
      echo "Physical input and all actions are disabled."
    else
      echo "Physical mode reads only the validated normalized InputPlumber target."
    fi

    identity_status="$(KORRID_PRIVATE_STATE_ROOT="$state_root" ${korridPackage}/bin/korrid identity status)"
    printf '%s\n' "$identity_status" \
      | jq -er 'select(._tag == "Unowned" or ._tag == "Owned" or ._tag == "Revoked") | .devicePublicKey | select(test("^[0-9a-f]{64}$"))' \
      > "$signer_credential_root/expected-device-public-key"
    chmod 0400 "$signer_credential_root/expected-device-public-key"
    # Setup has ended here, so the shell shows no setup and korrid never
    # restarts with exit 75 for setup's end, which would stop this runtime.
    # The device has no owner: no device makes a person key at start (ADR
    # 0082).
    install -m 0600 /dev/null "$state_root/onboarding-finished"

    systemd-socket-activate --now \
      -E "CREDENTIALS_DIRECTORY=$signer_credential_root" \
      -E "KORRI_LOCAL_SIGNER_PEER_UID=$(id -u)" \
      -E "KORRI_LOCAL_SIGNER_PEER_GID=$(id -g)" \
      -l "$signer_socket" \
      ${korridPackage}/bin/korri-local-signer &
    children+=("$!")
    for _ in $(seq 1 100); do
      [[ -S "$signer_socket" ]] && break
      kill -0 "''${children[0]}" 2>/dev/null || break
      sleep 0.01
    done
    [[ -S "$signer_socket" ]] || {
      echo "Korri local signer did not create its socket." >&2
      exit 1
    }

    KORRID_MODE=host \
      KORRID_ADDRESS=127.0.0.1:0 \
      KORRID_HOST_CONFIG="$runtime_root/host.toml" \
      KORRID_STORAGE_ROOT="$storage_root" \
      KORRID_PRIVATE_STATE_ROOT="$state_root" \
      KORRID_LOCAL_SIGNER_SOCKET="$signer_socket" \
      ${korridPackage}/bin/korrid &
    children+=("$!")

    KORRI_INPUTD_PROFILE=development \
      KORRI_INPUTD_SOURCE="$physical_input" \
      ${inputdPackage}/bin/korri-inputd &
    children+=("$!")

    set +e
    wait -n "''${children[@]}"
    status=$?
    set -e
    if [[ "$status" -eq 0 ]]; then
      echo "A Korri development process exited unexpectedly." >&2
      exit 1
    fi
    exit "$status"
  '';
}
