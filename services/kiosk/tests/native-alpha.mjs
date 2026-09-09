#!/usr/bin/env -S nix shell nixpkgs#nodejs nixpkgs#sway-unwrapped nixpkgs#grim nixpkgs#imagemagick nixpkgs#xwayland --command node
// Native compositor acceptance test, not a DevTools screenshot test.
// Usage: native-alpha.mjs /absolute/chromium [--opaque-control]
// The opaque control checks the patched browser with its new switch absent.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtemp, mkdir, writeFile, readdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, isAbsolute } from 'node:path';

const [chromium, mode] = process.argv.slice(2);
assert.ok(chromium && isAbsolute(chromium), 'Supply an absolute Chromium executable');
assert.ok(mode === undefined || mode === '--opaque-control', 'Unknown mode');
const alpha = mode !== '--opaque-control';
// Optional real Xwayland game. Its HOME is temporary; no user saves are touched.
const neverball = process.env.KORRI_ALPHA_NEVERBALL;
assert.ok(!neverball || isAbsolute(neverball), 'Neverball must be an absolute executable');
const root = await mkdtemp(join(tmpdir(), 'korri-native-alpha-'));
const socket = join(root, 'sway.sock');
const env = {
  ...process.env, XDG_RUNTIME_DIR: root, SWAYSOCK: socket,
  WLR_BACKENDS: 'headless', WLR_HEADLESS_OUTPUTS: '1', WLR_RENDERER: 'pixman',
};
const html = `<!doctype html><html><head><style>
html,body { margin:0;width:100%;height:100%;background:transparent }
#solid {position:absolute;left:0;top:0;width:25%;height:25%;background:#00ff00}
#blend {position:absolute;left:75%;top:75%;width:25%;height:25%;background:rgba(0,255,0,.5)}
</style></head><body><div id="solid"></div><div id="blend"></div></body></html>`;
const server = createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'text/html', 'Cache-Control': 'no-store' });
  res.end(req.url === '/blank' ? '<!doctype html><html></html>' : html);
});
await new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', resolve);
});
const origin = `http://127.0.0.1:${server.address().port}`;
await writeFile(join(root, 'sway.conf'), `xwayland ${neverball ? 'force' : 'disable'}
default_border none
for_window [shell="xwayland"] fullscreen disable, border none
floating_maximum_size -1 x -1
for_window [shell="xdg_shell"] floating enable, border none, resize set 640 480, move position 0 0
output HEADLESS-1 mode 640x480
output HEADLESS-1 bg #ff00ff solid_color
`);
let browser, sway, gamePid;
let gameFrame;
function underlay(x, y) {
  return gameFrame ? pixel(gameFrame, x, y) : 'srgb(255,0,255)';
}
function blendedGreen(background) {
  const channels = background.match(/^srgb\((\d+),(\d+),(\d+)\)$/)?.slice(1).map(Number);
  assert.ok(channels, `Unexpected compositor color ${background}`);
  // Alpha and each color channel can round independently to an 8-bit value.
  return [...new Set([127, 128].flatMap(alphaByte => Array.from({ length: 8 }, (_, rounding) => {
    const mixed = channels.map((value, index) => {
      const round = (rounding & (1 << index)) ? Math.ceil : Math.floor;
      return round((value * (255 - alphaByte) + (index === 1 ? 255 : 0) * alphaByte) / 255);
    });
    return `srgb(${mixed.join(',')})`;
  })))];
}
function shellQuote(value) {
  return "'" + value.replaceAll("'", "'\\''") + "'";
}
let sequence = 0;
let buffer = '';
const pending = new Map();
const logs = [];
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
function command(method, params = {}, sessionId) {
  const id = ++sequence;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`CDP timeout: ${method}`));
    }, 10000);
    pending.set(id, { resolve, reject, timer });
    browser.stdio[3].write(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }) + '\0');
  });
}
function swayCommand(...args) {
  return execFileSync('swaymsg', ['-s', socket, ...args], { env, encoding: 'utf8' });
}
function views(node) {
  return [...(node.pid ? [node] : []), ...(node.nodes ?? []).flatMap(views), ...(node.floating_nodes ?? []).flatMap(views)];
}
function pixel(path, x, y) {
  return execFileSync('magick', [path, '-format', `%[pixel:p{${x},${y}}]`, 'info:'], { encoding: 'utf8' });
}
async function capture(name, sample, expected) {
  const path = join(root, name + '.png');
  let actual;
  // Browser metadata can precede the first presented frame.
  for (let attempt = 0; attempt < 40; attempt++) {
    await delay(100);
    execFileSync('grim', ['-o', 'HEADLESS-1', path], { env });
    actual = pixel(path, ...sample);
    if ((Array.isArray(expected) ? expected : [expected]).includes(actual)) return path;
  }
  assert.fail(`Native compositor pixel at ${sample}: expected ${JSON.stringify(expected)}, got ${actual}; screenshot ${path}`);
}
try {
  sway = spawn('sway', ['--unsupported-gpu', '--config', join(root, 'sway.conf')], {
    env, stdio: ['ignore', 'ignore', 'pipe'],
  });
  sway.stderr.on('data', chunk => logs.push(chunk.toString()));
  for (let attempt = 0; attempt < 100; attempt++) {
    env.WAYLAND_DISPLAY = (await readdir(root)).find(name => /^wayland-\d+$/.test(name));
    if (env.WAYLAND_DISPLAY) break;
    assert.equal(sway.exitCode, null, 'Sway exited before creating its socket');
    await delay(50);
  }
  assert.ok(env.WAYLAND_DISPLAY, 'No Wayland socket');
  if (neverball) {
    const gameHome = join(root, 'game-home');
    await mkdir(gameHome, { mode: 0o700 });
    const reply = JSON.parse(swayCommand('exec', `env HOME=${shellQuote(gameHome)} XDG_CONFIG_HOME=${shellQuote(gameHome)} SDL_VIDEODRIVER=x11 SDL_AUDIODRIVER=dummy LIBGL_ALWAYS_SOFTWARE=1 ${shellQuote(neverball)}`));
    assert.ok(reply.every(result => result.success), 'Could not start Neverball');
    for (let attempt = 0; attempt < 150; attempt++) {
      const game = views(JSON.parse(swayCommand('-t', 'get_tree', '-r'))).find(view => view.shell === 'xwayland');
      if (game) { gamePid = game.pid; break; }
      await delay(100);
    }
    assert.ok(Number.isSafeInteger(gamePid) && gamePid > 1, 'No Neverball Xwayland window');
    await delay(1500);
    // Freeze only this disposable fixture to compare identical game pixels.
    // Production pause/resume stays in korrid, never in this test helper.
    process.kill(gamePid, 'SIGSTOP');
    await delay(300);
    gameFrame = join(root, 'game-underlay.png');
    execFileSync('grim', ['-o', 'HEADLESS-1', gameFrame], { env });
    console.log('Real game underlay:', gameFrame);
  }
  browser = spawn(chromium, [
    '--ozone-platform=wayland', '--remote-debugging-pipe', '--no-first-run',
    '--no-default-browser-check', '--disable-extensions', '--disable-default-apps',
    '--use-gl=angle', '--use-angle=gles-egl',
    `--user-data-dir=${join(root, 'profile')}`, `--app=${origin}/blank`,
    ...(alpha ? ['--enable-transparent-visuals'] : []),
  ], { env, detached: true, stdio: ['ignore', 'ignore', 'pipe', 'pipe', 'pipe'] });
  browser.stderr.on('data', chunk => logs.push(chunk.toString()));
  browser.stdio[4].on('data', chunk => {
    buffer += chunk.toString();
    let end;
    while ((end = buffer.indexOf('\0')) >= 0) {
      const frame = JSON.parse(buffer.slice(0, end));
      buffer = buffer.slice(end + 1);
      const request = pending.get(frame.id);
      if (request) {
        clearTimeout(request.timer);
        pending.delete(frame.id);
        if (frame.error) request.reject(new Error(JSON.stringify(frame.error)));
        else request.resolve(frame.result);
      }
    }
  });
  console.log('Browser:', await command('Browser.getVersion'));
  let page;
  for (let attempt = 0; attempt < 100; attempt++) {
    const pages = (await command('Target.getTargets')).targetInfos.filter(target => target.type === 'page');
    assert.ok(pages.length <= 1, 'Expected one page');
    if (pages[0]?.url === origin + '/blank') { page = pages[0]; break; }
    await delay(50);
  }
  assert.ok(page, 'App bootstrap did not load');
  const { sessionId } = await command('Target.attachToTarget', { targetId: page.targetId, flatten: true });
  await command('Page.enable', {}, sessionId);
  // Exercise CSS alone. Production should not need a DevTools screenshot or
  // emulation override to make the native window transparent.
  await command('Page.navigate', { url: origin + '/' }, sessionId);
  // Solid green proves that the page, not a tab/address bar, reaches the top.
  await capture('ready', [40, 20], 'srgb(0,255,0)');
  const nativeViews = views(JSON.parse(swayCommand('-t', 'get_tree', '-r')));
  assert.equal(nativeViews.length, neverball ? 2 : 1);
  const browserViews = nativeViews.filter(view => view.shell === 'xdg_shell');
  assert.equal(browserViews.length, 1);
  const view = browserViews[0];
  assert.equal(view.fullscreen_mode, 0);
  assert.equal(view.shell, 'xdg_shell', 'This test must not use Xwayland');
  for (const [width, height] of [[640, 480], [480, 360]]) {
    const result = JSON.parse(swayCommand(`[con_id=${view.id}] floating enable, border none, resize set ${width} ${height}, move position 0 0`));
    assert.ok(result.every(reply => reply.success));
    const center = [Math.floor(width / 2), Math.floor(height / 2)];
    const blendPoint = [Math.floor(width * .875), Math.floor(height * .875)];
    const background = alpha ? underlay(...center) : 'srgb(255,255,255)';
    let path = await capture(`overlay-${width}`, center, background);
    // In both modes, wait for resized content, not an unchanged center pixel.
    const blendedColors = blendedGreen(alpha ? underlay(...blendPoint) : 'srgb(255,255,255)');
    path = await capture(`blend-${width}`, blendPoint, blendedColors);
    assert.equal(pixel(path, ...center), background);
    assert.equal(pixel(path, 40, 20), 'srgb(0,255,0)');
    const resized = views(JSON.parse(swayCommand('-t', 'get_tree', '-r'))).find(node => node.id === view.id);
    assert.deepEqual(resized.rect, { x: 0, y: 0, width, height });
  }
  // Use the same page/browser for the opaque hub, then the overlay again.
  await command('Runtime.evaluate', { expression: "document.body.style.background='#0000ff'" }, sessionId);
  await capture('opaque-hub', [240, 180], 'srgb(0,0,255)');
  await command('Runtime.evaluate', { expression: "document.body.style.background='transparent'" }, sessionId);
  await capture('overlay-return', [240, 180], alpha ? underlay(240, 180) : 'srgb(255,255,255)');
  assert.equal(views(JSON.parse(swayCommand('-t', 'get_tree', '-r'))).length, neverball ? 2 : 1);
  if (gamePid) {
    const signature = path => execFileSync('magick', [path, '-crop', '160x480+480+0', '-depth', '8', 'rgb:-']);
    const frozenSignature = signature(gameFrame);
    process.kill(gamePid, 'SIGCONT');
    const resumed = join(root, 'game-resumed.png');
    let changed = false;
    for (let attempt = 0; attempt < 30; attempt++) {
      await delay(100);
      execFileSync('grim', ['-o', 'HEADLESS-1', resumed], { env });
      if (!signature(resumed).equals(frozenSignature)) { changed = true; break; }
    }
    assert.ok(changed, 'Neverball must resume rendering while the overlay stays open');
    console.log('PASS: Neverball resumed rendering behind the persistent overlay');
  }
  console.log(`PASS: ${alpha ? 'native alpha and blending' : 'opaque default'}; one native nonfullscreen window; resize and hub/overlay reuse`);
} finally {
  for (const request of pending.values()) clearTimeout(request.timer);
  if (browser?.pid) {
    try { process.kill(-browser.pid, 'SIGTERM'); } catch {}
    await delay(300);
    try { process.kill(-browser.pid, 'SIGKILL'); } catch {}
  }
  if (gamePid) {
    try { process.kill(gamePid, 'SIGCONT'); } catch {}
    try { process.kill(gamePid, 'SIGTERM'); } catch {}
  }
  sway?.kill('SIGTERM');
  server.closeAllConnections();
  server.close();
  await writeFile(join(root, 'browser-sway.log'), logs.join(''));
  console.log('Evidence:', root);
}
