{ pkgs, inputdPackage }:
pkgs.runCommand "korri-input-seat-receiver-check"
  {
    nativeBuildInputs = [ pkgs.python3 ];
  }
  ''
      set -euo pipefail
      test -x ${inputdPackage}/bin/korri-input-seat-receiver
      runtime="$TMPDIR/runtime"
      mkdir -m 700 "$runtime"
      uid="$(id -u)"
      gid="$(id -g)"
      ${inputdPackage}/bin/korri-input-seat-receiver \
        --runtime-dir "$runtime" \
        --control-uid "$uid" \
        --control-gid "$gid" \
        --sunshine-uid "$uid" \
        --sunshine-gid "$gid" \
        --event-gid "$gid" \
        --dry-run \
        >"$TMPDIR/receiver.out" 2>"$TMPDIR/receiver.err" &
      receiver=$!
      trap 'kill "$receiver" 2>/dev/null || true; wait "$receiver" 2>/dev/null || true; cat "$TMPDIR/receiver.err" >&2' EXIT
      for _ in $(seq 1 100); do
        test -S "$runtime/control.sock" && break
        sleep 0.01
      done
      test -S "$runtime/control.sock"
      python3 - "$runtime" <<'PY'
    import json, socket, sys, time
    from pathlib import Path
    root = Path(sys.argv[1])
    launch = "0123456789abcdef0123456789abcdef"
    def request(op): return bytes([1, op]) + launch.encode()
    coordinator = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
    coordinator.settimeout(2)
    coordinator.connect(str(root / "control.sock"))
    def coordinate(operation, **fields):
        coordinator.sendall(bytes([2]) + json.dumps(dict(operation=operation, **fields)).encode())
        packet = coordinator.recv(65536)
        assert packet[0] == 2
        reply = json.loads(packet[1:])
        assert reply['failure'] is None, reply
        return reply
    coordinate('hello')
    assert coordinate('applyCount', count=6)['count'] == 6
    neutral = dict(buttons=0, left_trigger=0, right_trigger=0, left_stick_x=0,
                   left_stick_y=0, right_stick_x=0, right_stick_y=0)
    assert coordinate('physicalConnected', deviceId='fixture-physical', name='Fixture controller', state=neutral)['slot'] == 1
    coordinate('beginSession', launchId=launch)
    control = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
    control.settimeout(2)
    control.connect(str(root / "control.sock"))
    control.sendall(request(1))
    assert control.recv(3) == bytes([1, 0, 0])
    for _ in range(100):
        if (root / "sunshine-active-launch.json").is_file(): break
        time.sleep(0.01)
    sidecar = json.loads((root / "sunshine-active-launch.json").read_text())
    assert set(sidecar) == {"launchId", "generation", "mirrorToken"}
    assert sidecar["launchId"] == launch
    assert len(sidecar["mirrorToken"]) == 64
    mirror = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
    mirror.connect(str(root / "sunshine-input-seat.sock"))
    # The first measured state establishes a baseline. A connection announcement
    # alone must not invent neutral state or produce browser input.
    frame = {"mirrorToken": sidecar["mirrorToken"], "frame": {
        "kind": "source-state", "launchId": launch, "controllerNumber": 0,
        "buttons": 0, "leftTrigger": 0, "rightTrigger": 0,
        "leftStickX": 0, "leftStickY": 0, "rightStickX": 0, "rightStickY": 0,
    }}
    mirror.sendall((json.dumps(frame, separators=(",", ":")) + "\n").encode())
    mirror.close()
    for _ in range(100):
        reply = coordinate('poll')
        if reply['remoteSources']:
            break
        time.sleep(0.01)
    assert len(reply['remoteSources']) == 1, 'remote baseline did not arrive'
    assert reply['remoteSources'][0]['slot'] == 2
    assert reply['remoteSources'][0]['controllerNumber'] == 0
    control.sendall(request(2))
    assert control.recv(3) == bytes([1, 0, 0])
    control.close()
    for _ in range(100):
        if not (root / "sunshine-active-launch.json").exists(): break
        time.sleep(0.01)
    assert not (root / "sunshine-active-launch.json").exists()
    assert not (root / "sunshine-input-seat.sock").exists()
    # STOP revokes mirror authority, not authoritative session reservations.
    reply = coordinate('poll')
    assert reply['session'] == launch
    assert reply['remoteSources'] == []
    assert reply['count'] == 6
    coordinate('endSession', launchId=launch)
    coordinate('physicalDisconnected', deviceId='fixture-physical')
    assert coordinate('poll')['count'] == 6
    coordinator.close()
    PY
      kill -TERM "$receiver"
      wait "$receiver"
      trap - EXIT
      touch "$out"
  ''
