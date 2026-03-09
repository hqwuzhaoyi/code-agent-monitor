const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadMacBridge() {
  const filePath = path.join(__dirname, 'mac-bridge.js');
  const source = fs.readFileSync(filePath, 'utf8');
  const instrumented = `${source.replace(/\nconnect\(\);\s*$/, '\n')}
module.exports = {
  buildCamStartCommand,
  parseCamStartResult,
};`;

  class FakeWebSocket {
    constructor() {
      this.readyState = FakeWebSocket.OPEN;
    }
    on() {}
    send() {}
    close() {}
  }
  FakeWebSocket.OPEN = 1;

  const sandbox = {
    module: { exports: {} },
    exports: {},
    require: (name) => {
      if (name === 'ws') {
        return FakeWebSocket;
      }
      if (name === 'child_process') {
        return {
          exec: (_command, _options, callback) => callback(null, '', ''),
          execSync: () => '',
          spawn: () => ({ on() {}, kill() {} }),
        };
      }
      if (name === 'os') {
        return {
          homedir: () => '/Users/test',
          hostname: () => 'test-mac.local',
          networkInterfaces: () => ({}),
          totalmem: () => 8 * 1024 * 1024 * 1024,
          freemem: () => 4 * 1024 * 1024 * 1024,
          cpus: () => [{ model: 'Fake CPU' }],
          uptime: () => 60,
        };
      }
      return require(name);
    },
    process: {
      argv: ['node', 'mac-bridge.js'],
      env: {},
      on() {},
      exit() {},
    },
    console,
    global: {},
    setInterval: () => 0,
    clearInterval: () => {},
    setTimeout,
    clearTimeout,
    Buffer,
    __dirname: path.dirname(filePath),
    __filename: filePath,
  };

  vm.runInNewContext(instrumented, sandbox, { filename: filePath });
  return sandbox.module.exports;
}

test('buildCamStartCommand includes --json output flag', () => {
  const { buildCamStartCommand } = loadMacBridge();
  const command = buildCamStartCommand('/tmp/project', 'cam-test');

  assert.match(command, /^cam start /);
  assert.match(command, /--cwd "\/tmp\/project"/);
  assert.match(command, /--name cam-test/);
  assert.match(command, /--json/);
});

test('parseCamStartResult returns agent identity from JSON stdout', () => {
  const { parseCamStartResult } = loadMacBridge();
  const parsed = parseCamStartResult(
    JSON.stringify({
      agent_id: 'cam-123',
      tmux_session: 'cam-session',
      agent_type: 'claude',
      project_path: '/tmp/project',
    }),
    '',
    0,
    '/tmp/project',
    'cam-session',
    'hello'
  );

  assert.equal(parsed.status, 'started');
  assert.equal(parsed.agent_id, 'cam-123');
  assert.equal(parsed.tmux_session, 'cam-session');
  assert.equal(parsed.session, 'cam-session');
  assert.equal(parsed.working_directory, '/tmp/project');
});
