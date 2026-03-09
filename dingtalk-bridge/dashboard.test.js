const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadDashboard() {
  const filePath = path.join(__dirname, 'dashboard.html');
  const html = fs.readFileSync(filePath, 'utf8');
  const match = html.match(/<script>([\s\S]*)<\/script>\s*<\/body>/);
  if (!match) throw new Error('dashboard script not found');
  const script = `${match[1]}\nmodule.exports = { startNewAgent };`;

  const elements = new Map();
  const makeElement = (id) => ({
    id,
    value: '',
    textContent: '',
    disabled: false,
    style: {},
    className: '',
    focus() {},
    addEventListener() {},
    removeEventListener() {},
    appendChild() {},
    querySelector() { return null; },
    querySelectorAll() { return []; },
    classList: { add() {}, remove() {}, contains() { return false; } },
    getContext() { return {}; },
  });

  const document = {
    body: { classList: { add() {}, remove() {} } },
    addEventListener() {},
    querySelector(selector) {
      if (selector === '.agent-form-submit') return this.getElementById('agent-form-submit');
      return null;
    },
    querySelectorAll() { return []; },
    getElementById(id) {
      if (!elements.has(id)) elements.set(id, makeElement(id));
      return elements.get(id);
    },
  };

  const toasts = [];
  let fetchImpl = async () => ({ json: async () => ({ ok: true }) });
  let hideCalls = 0;

  const sandbox = {
    module: { exports: {} },
    exports: {},
    document,
    window: { addEventListener() {}, innerWidth: 1280, location: { hash: '' } },
    location: { hash: '' },
    history: { pushState() {}, replaceState() {} },
    navigator: { userAgent: 'node-test' },
    localStorage: { getItem() { return null; }, setItem() {}, removeItem() {} },
    sessionStorage: { getItem() { return null; }, setItem() {}, removeItem() {} },
    EventSource: function() {},
    WebSocket: function() {},
    marked: { parse: (s) => s },
    mermaid: { initialize() {}, run: async () => {} },
    hljs: { highlightElement() {} },
    fetch: (...args) => fetchImpl(...args),
    setTimeout,
    clearTimeout,
    setInterval: () => 0,
    clearInterval() {},
    console,
  };

  vm.runInNewContext(script, sandbox, { filename: filePath });

  sandbox.showToast = (msg, type) => { toasts.push({ msg, type }); };
  sandbox.hideNewAgentForm = () => { hideCalls += 1; };

  return {
    startNewAgent: sandbox.module.exports.startNewAgent,
    document,
    toasts,
    getHideCalls: () => hideCalls,
    setFetch(fn) { fetchImpl = fn; },
  };
}

test('startNewAgent shows accepted async message instead of immediate started message', async () => {
  const app = loadDashboard();
  app.document.getElementById('newAgentCwd').value = '~/project/demo';
  app.document.getElementById('newAgentName').value = 'cam-demo';
  app.document.getElementById('newAgentPrompt').value = 'do work';

  app.setFetch(async () => ({
    json: async () => ({
      ok: true,
      accepted: true,
      message: '已提交到 Mac 执行，稍后会出现在列表中',
      agent: { agent_id: 'cam-123', tmux_session: 'cam-123' },
    }),
  }));

  await app.startNewAgent();

  assert.equal(app.getHideCalls(), 1);
  assert.equal(app.toasts.length, 1);
  assert.match(app.toasts[0].msg, /已提交到 Mac/);
  assert.doesNotMatch(app.toasts[0].msg, /会话已启动/);
});
