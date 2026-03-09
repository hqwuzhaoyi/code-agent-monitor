const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { EventEmitter } = require('node:events');

function loadServer() {
  const filePath = path.join(__dirname, 'server.js');
  const source = fs.readFileSync(filePath, 'utf8');
  const instrumented = source.replace(
    /heartbeatServer\.listen\([\s\S]*$/,
    `module.exports = { heartbeatHandler: heartbeatServer.listeners('request')[0], buildStartAgentAcceptedResponse };\n`
  );

  let requestHandler = null;
  const fakeServer = {
    listen() {},
    on() {},
    listeners(event) { return event === 'request' && requestHandler ? [requestHandler] : []; },
  };

  class FakeWSS {
    on() {}
  }

  const sandbox = {
    module: { exports: {} },
    exports: {},
    __dirname: path.dirname(filePath),
    __filename: filePath,
    require(name) {
      if (name === 'http') {
        return {
          createServer(handler) {
            requestHandler = handler;
            return fakeServer;
          },
          request() { throw new Error('unexpected http.request'); },
        };
      }
      if (name === 'https') return { request() { throw new Error('unexpected https.request'); } };
      if (name === 'ws') return { WebSocketServer: FakeWSS };
      if (name === 'dingtalk-stream') return {
        DWClient: class {
          registerCallbackListener() {}
          socketCallBackResponse() {}
          start() { return Promise.resolve(); }
        },
        TOPIC_ROBOT: 'robot'
      };
      if (name === 'crypto') return require('node:crypto');
      return require(name);
    },
    process: {
      env: {
        DINGTALK_CLIENT_ID: 'id',
        DINGTALK_CLIENT_SECRET: 'secret',
        AI_API_KEY: 'key',
      },
      exit(code) { throw new Error(`process.exit(${code})`); },
    },
    console,
    setTimeout,
    clearTimeout,
    setInterval: () => 0,
    clearInterval() {},
    Buffer,
  };

  vm.runInNewContext(instrumented, sandbox, { filename: filePath });
  return sandbox;
}

async function invokeJson(handler, { method, url, body }) {
  const req = new EventEmitter();
  req.method = method;
  req.url = url;
  const headers = {};
  let statusCode = null;
  let responseBody = '';
  const res = {
    setHeader(name, value) { headers[name] = value; },
    writeHead(code, extraHeaders = {}) { statusCode = code; Object.assign(headers, extraHeaders); },
    end(chunk = '') { responseBody += chunk; },
  };

  handler(req, res);
  if (body !== undefined) {
    req.emit('data', Buffer.from(body));
  }
  req.emit('end');
  await new Promise(resolve => setImmediate(resolve));

  return { statusCode, headers, json: JSON.parse(responseBody) };
}

test('buildStartAgentAcceptedResponse returns accepted response shape for async start', () => {
  const sandbox = loadServer();
  const { buildStartAgentAcceptedResponse } = sandbox.module.exports;

  const response = buildStartAgentAcceptedResponse(
    { name: 'mac-mini' },
    {
      status: 'started',
      agent_id: 'cam-123',
      tmux_session: 'cam-123',
      session: 'cam-123',
    }
  );

  assert.equal(response.ok, true);
  assert.equal(response.accepted, true);
  assert.equal(response.device, 'mac-mini');
  assert.equal(response.agent.agent_id, 'cam-123');
  assert.equal(response.agent.tmux_session, 'cam-123');
  assert.match(response.message, /已提交到 Mac/);
  assert.equal(response.result, undefined);
});
