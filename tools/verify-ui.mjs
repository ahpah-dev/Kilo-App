/**
 * Verify the built 1.0.6 frontend in a real browser over CDP.
 *
 * Runs against the production dist/ bundle, so this exercises the same code the
 * installer ships. Uses Node's built-in WebSocket (Node >= 22).
 */
import { spawn } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';

const EDGE = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const PORT = 9222;
const BASE = 'http://127.0.0.1:5180';
const PROFILE = `${process.env.TEMP}\\edge-cdp-${Date.now()}`;

const seed = [
  {
    id: 'c-auth', title: 'Auth refactor', model: 'kilo-auto/free',
    created_at: '2026-10-13T20:00:00.000Z', updated_at: '2026-10-13T20:05:00.000Z',
    messages: [
      { id: 'a1', role: 'user', content: 'Refactor the login flow to use the new token store.', created_at: '2026-10-13T20:00:00.000Z' },
      { id: 'a2', role: 'assistant', content: 'Done. The session token now lives in the keyring.', created_at: '2026-10-13T20:01:00.000Z' },
    ],
  },
  {
    id: 'c-rate', title: 'Rate limit handling', model: 'kilo-auto/free',
    created_at: '2026-10-13T20:02:00.000Z', updated_at: '2026-10-13T20:03:00.000Z',
    messages: [
      { id: 'b1', role: 'user', content: 'Add automatic retry when the gateway rate limits us.', created_at: '2026-10-13T20:02:00.000Z' },
      { id: 'b2', role: 'assistant', content: 'Added a 10s then 20s backoff for HTTP 429 rate limits.', created_at: '2026-10-13T20:03:00.000Z' },
    ],
  },
];

const results = {};
const consoleErrors = [];

class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.addEventListener('message', (event) => {
      const msg = JSON.parse(event.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result ?? {});
      } else if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') {
        consoleErrors.push(msg.params.args.map((a) => a.value ?? a.description).join(' '));
      } else if (msg.method === 'Runtime.exceptionThrown') {
        consoleErrors.push(msg.params.exceptionDetails.text + ' ' +
          (msg.params.exceptionDetails.exception?.description ?? ''));
      }
    });
  }

  send(method, params = {}) {
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      setTimeout(() => {
        if (this.pending.delete(id)) reject(new Error(`${method} timed out`));
      }, 15000);
    });
  }

  async eval(expression) {
    const res = await this.send('Runtime.evaluate', {
      expression, returnByValue: true, awaitPromise: true,
    });
    if (res.exceptionDetails) throw new Error(res.exceptionDetails.text);
    return res.result?.value;
  }

  async key(k, code, vk, modifiers = 0) {
    for (const type of ['keyDown', 'keyUp']) {
      await this.send('Input.dispatchKeyEvent', {
        type, modifiers, key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk,
      });
    }
  }

  async click(selector) {
    return this.eval(`(() => { const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return false; el.click(); return true; })()`);
  }
}

const edge = spawn(EDGE, [
  '--headless=new', '--disable-gpu', `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${PROFILE}`, '--no-first-run', '--window-size=1280,860', 'about:blank',
], { stdio: 'ignore', detached: false });

async function findTarget() {
  for (let i = 0; i < 60; i++) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
      const list = await res.json();
      const page = list.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
      if (page) return page;
    } catch { /* not up yet */ }
    await sleep(500);
  }
  throw new Error('no CDP page target');
}

try {
  const target = await findTarget();
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });
  const cdp = new Cdp(ws);
  await cdp.send('Runtime.enable');
  await cdp.send('Page.enable');

  // Seed storage on the app's own origin, then load the app.
  await cdp.send('Page.navigate', { url: `${BASE}/__seed.html` });
  await sleep(2500);
  results['0_seeded'] = await cdp.eval("localStorage.getItem('kilo-app-conversations')?.length > 0");
  await cdp.send('Page.navigate', { url: `${BASE}/index.html` });
  await sleep(3500);

  results['1_app_loaded'] = await cdp.eval("!!document.querySelector('.app-shell')");
  results['2_conversations'] = await cdp.eval("document.querySelectorAll('.sidebar .conv').length");
  results['3_nav_labels'] = await cdp.eval(
    "[...document.querySelectorAll('.sidebar .nav-item .nav-label')].map(e => e.textContent.trim())"
  );
  results['4_palette_hint'] = await cdp.eval("!!document.querySelector('.palette-hint')");

  // --- Palette: open via the sidebar hint -----------------------------------
  await cdp.click('.palette-hint');
  await sleep(700);
  results['5_palette_opens_on_click'] = await cdp.eval("!!document.querySelector('.palette')");
  results['6_rows'] = await cdp.eval("document.querySelectorAll('.palette .row').length");
  results['7_groups'] = await cdp.eval(
    "[...document.querySelectorAll('.palette .group-label')].map(e => e.textContent.trim())"
  );

  // --- Palette: filtering ---------------------------------------------------
  await cdp.eval(`(() => { const i = document.querySelector('.palette input');
    i.value = 'rate'; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
  await sleep(400);
  results['8_filtered'] = await cdp.eval(
    "[...document.querySelectorAll('.palette .row .row-label')].map(e => e.textContent.trim())"
  );

  // --- Palette: Ctrl+K toggle and Escape ------------------------------------
  await cdp.key('Escape', 'Escape', 27);
  await sleep(300);
  results['9_escape_closes'] = await cdp.eval("!document.querySelector('.palette')");
  await cdp.key('k', 'KeyK', 75, 2);
  await sleep(600);
  results['10_ctrl_k_opens'] = await cdp.eval("!!document.querySelector('.palette')");

  // --- Palette: navigate to Search -----------------------------------------
  await cdp.eval(`(() => { const i = document.querySelector('.palette input');
    i.value = 'search all'; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
  await sleep(400);
  await cdp.key('Enter', 'Enter', 13);
  await sleep(1200);
  results['11_search_route'] = await cdp.eval("!!document.querySelector('.search-view')");

  // --- Search: filtering and highlight -------------------------------------
  if (results['11_search_route']) {
    await cdp.eval(`(() => { const i = document.querySelector('.search-bar input');
      i.value = 'backoff'; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
    await sleep(800);
    results['12_hits'] = await cdp.eval("document.querySelectorAll('.search-view .hit').length");
    results['13_highlight'] = await cdp.eval(
      "document.querySelector('.search-view .hit mark')?.textContent ?? null"
    );
    await cdp.eval(`(() => { const i = document.querySelector('.search-bar input');
      i.value = 'zzznotpresent'; i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
    await sleep(800);
    results['14_empty_state'] = await cdp.eval("!!document.querySelector('.search-view .hint-state')");
  }

  // --- Sidebar: per-chat menu and pinning ----------------------------------
  await cdp.click('.sidebar .nav-item[href="/chat"]');
  await sleep(900);
  await cdp.click('.sidebar .conv .conv-menu-btn');
  await sleep(500);
  results['15_menu_opens'] = await cdp.eval("!!document.querySelector('.sidebar .conv-menu')");
  results['16_menu_items'] = await cdp.eval(
    "[...document.querySelectorAll('.sidebar .conv-menu button')].map(e => e.textContent.trim())"
  );
  await cdp.eval(`(() => { const b = [...document.querySelectorAll('.sidebar .conv-menu button')]
    .find(x => x.textContent.trim() === 'Pin'); if (b) b.click(); return !!b; })()`);
  await sleep(700);
  results['17_groups_after_pin'] = await cdp.eval(
    "[...document.querySelectorAll('.sidebar .group-label')].map(e => e.textContent.trim())"
  );
  results['18_pin_icon'] = await cdp.eval("!!document.querySelector('.sidebar .conv-pin')");

  // --- Commands present in the palette (models, settings, themes) ----------
  await cdp.click('.palette-hint');
  await sleep(600);
  results['19_command_groups'] = await cdp.eval(
    "[...new Set([...document.querySelectorAll('.palette .group-label')].map(e => e.textContent.trim()))]"
  );
  results['20_theme_commands'] = await cdp.eval(
    "[...document.querySelectorAll('.palette .row .row-label')].filter(e => e.textContent.startsWith('Theme:')).length"
  );

  results['21_console_errors'] = consoleErrors;
  console.log(JSON.stringify(results, null, 2));
  ws.close();
} catch (error) {
  console.error('VERIFY FAILED:', error.message);
  console.log(JSON.stringify({ ...results, consoleErrors }, null, 2));
  process.exitCode = 1;
} finally {
  edge.kill();
}
