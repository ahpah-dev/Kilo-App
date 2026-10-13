/** Capture screenshots of the new 1.0.6 UI from the built bundle. */
import { spawn } from 'node:child_process';
import { writeFileSync, mkdirSync } from 'node:fs';
import { setTimeout as sleep } from 'node:timers/promises';

const EDGE = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const PORT = 9223;
const BASE = 'http://127.0.0.1:5180';
const PROFILE = `${process.env.TEMP}\\edge-shot-${Date.now()}`;
const OUT = 'tools/out';
mkdirSync(OUT, { recursive: true });

class Cdp {
  constructor(ws) {
    this.ws = ws; this.id = 0; this.pending = new Map();
    ws.addEventListener('message', (e) => {
      const m = JSON.parse(e.data);
      if (m.id && this.pending.has(m.id)) {
        const { resolve, reject } = this.pending.get(m.id);
        this.pending.delete(m.id);
        m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result ?? {});
      }
    });
  }
  send(method, params = {}) {
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((res, rej) => {
      this.pending.set(id, { resolve: res, reject: rej });
      setTimeout(() => { if (this.pending.delete(id)) rej(new Error(`${method} timeout`)); }, 20000);
    });
  }
  async eval(expression) {
    const r = await this.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    return r.result?.value;
  }
  async key(k, code, vk, modifiers = 0) {
    for (const type of ['keyDown', 'keyUp']) {
      await this.send('Input.dispatchKeyEvent', {
        type, modifiers, key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk });
    }
  }
  async shot(path) {
    const r = await this.send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(path, Buffer.from(r.data, 'base64'));
    console.log('captured', path);
  }
}

const edge = spawn(EDGE, [
  '--headless=new', '--disable-gpu', `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${PROFILE}`, '--no-first-run', '--window-size=1280,820', 'about:blank',
], { stdio: 'ignore' });

try {
  let page;
  for (let i = 0; i < 60 && !page; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      page = list.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
    } catch { /* not ready */ }
    if (!page) await sleep(500);
  }
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => {
    ws.addEventListener('open', res, { once: true });
    ws.addEventListener('error', rej, { once: true });
  });
  const cdp = new Cdp(ws);
  await cdp.send('Runtime.enable');
  await cdp.send('Page.enable');
  await cdp.send('Emulation.setDeviceMetricsOverride', {
    width: 1280, height: 820, deviceScaleFactor: 1, mobile: false,
  });

  await cdp.send('Page.navigate', { url: `${BASE}/__seed.html?theme=dark&ui=glass` });
  await sleep(3000);

  // 1. Command palette, empty query.
  await cdp.eval("document.querySelector('.palette-hint').click()");
  await sleep(900);
  await cdp.shot(`${OUT}/feat-palette.png`);

  // 2. Palette with a query.
  await cdp.eval(`(() => { const i = document.querySelector('.palette input');
    i.value = 'theme'; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
  await sleep(500);
  await cdp.shot(`${OUT}/feat-palette-filtered.png`);

  // 3. Search view with results.
  await cdp.key('Escape', 'Escape', 27);
  await sleep(300);
  await cdp.eval("document.querySelector('.palette-hint').click()");
  await sleep(500);
  await cdp.eval(`(() => { const i = document.querySelector('.palette input');
    i.value = 'search all'; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
  await sleep(400);
  await cdp.key('Enter', 'Enter', 13);
  await sleep(1200);
  await cdp.eval(`(() => { const i = document.querySelector('.search-bar input');
    i.value = 'rate'; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
  await sleep(700);
  await cdp.shot(`${OUT}/feat-search.png`);

  // 4. Sidebar chat menu.
  await cdp.eval(`(() => { const a = document.querySelector('.sidebar .nav-item[href="/chat"]');
    if (a) a.click(); return true; })()`);
  await sleep(900);
  await cdp.eval("document.querySelector('.sidebar .conv .conv-menu-btn')?.click()");
  await sleep(600);
  await cdp.shot(`${OUT}/feat-menu.png`);

  ws.close();
} catch (e) {
  console.error('CAPTURE FAILED:', e.message);
  process.exitCode = 1;
} finally {
  edge.kill();
}
