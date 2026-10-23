#!/usr/bin/env nix
#! nix shell nixpkgs#bash nixpkgs#sway nixpkgs#python3 --command bash
# shellcheck shell=bash
set -euo pipefail
cd "$(dirname "$0")"
launcher="$(./fuji-build.sh | tail -1)/bin/korri-portal-wpe"
runtime="$(mktemp -d /tmp/korri-wpe-wayland-XXXXXX)"
export XDG_RUNTIME_DIR="$runtime"
export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
printf 'output * resolution 640x480\n' > "$runtime/sway.conf"
WAYLAND_DEBUG=server sway --unsupported-gpu --config "$runtime/sway.conf" > "$runtime/sway.log" 2>&1 &
compositor=$!
cleanup() {
  kill "$compositor" 2>/dev/null || true
  wait "$compositor" 2>/dev/null || true
}
trap cleanup EXIT
for _ in {1..100}; do
  sockets=("$runtime"/sway-ipc.*.sock)
  if [[ -S "${sockets[0]}" ]]; then break; fi
  sleep .1
done
export SWAYSOCK="${sockets[0]}"
export WAYLAND_DISPLAY=wayland-1
python3 - "$launcher" <<'PY'
import http.server, json, os, pathlib, subprocess, sys, threading, time
root = pathlib.Path(os.environ['XDG_RUNTIME_DIR'])
(root / 'KORRID_RPC_CAPABILITY').write_text('wayland-test-capability')
(root / 'KORRID_RPC_CAPABILITY').chmod(0o600)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.end_headers()
        self.wfile.write(b'<script>KorriRpc.korridPort();KorriRpc.korridCapability()</script>')
    def log_message(self, *_): pass
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
origin = f'http://127.0.0.1:{server.server_port}'
env = dict(os.environ, HOME=str(root), WPE_PLATFORM='wayland', CREDENTIALS_DIRECTORY=str(root),
           KORRID_ADDRESS='127.0.0.1:7887', KORRID_PORTAL_ORIGIN=origin, KORRI_WEB_SURFACE_URL=origin+'/')
mesa = subprocess.check_output(['nix', 'eval', '--raw', 'nixpkgs#mesa.outPath'], text=True)
env.update(GBM_BACKENDS_PATH=mesa+'/lib/gbm', LIBGL_DRIVERS_PATH=mesa+'/lib/dri',
           __EGL_VENDOR_LIBRARY_FILENAMES=mesa+'/share/glvnd/egl_vendor.d/50_mesa.json')
process = subprocess.Popen([sys.argv[1]], env=env, start_new_session=True)
try:
    def windows(node):
        yield node
        for child in node.get('nodes', []) + node.get('floating_nodes', []): yield from windows(child)
    for _ in range(100):
        tree = json.loads(subprocess.check_output(['swaymsg', '-t', 'get_tree', '-r']))
        matches = [node for node in windows(tree) if node.get('app_id') == 'org.korri.portal']
        protocol = (root / 'sway.log').read_text()
        if 'set_app_id("org.korri.portal")' in protocol and 'set_fullscreen(' in protocol:
            print('VERIFIED Wayland requests: app_id=org.korri.portal, set_fullscreen')
            print('Mapped compositor windows:', len(matches))
            break
        if process.poll() is not None: raise RuntimeError(f'launcher exited {process.returncode}')
        time.sleep(.1)
    else: raise AssertionError('No Wayland window')
finally:
    process.terminate(); process.wait(timeout=10)
    server.shutdown()
PY
