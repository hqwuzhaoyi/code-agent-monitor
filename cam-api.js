/**
 * CAM HTTP API - 暴露 cam 命令为 HTTP 接口
 * 供 NAS 上的钉钉机器人调用
 * 
 * 端口: 18790
 * 认证: Bearer token
 */
const http = require('http');
const { exec } = require('child_process');
const os = require('os');

const PORT = 18790;
const AUTH_TOKEN = 'cam-dingtalk-bridge-2026';
const CAM_PATH = `${os.homedir()}/.local/bin/cam`;

function runCam(args, timeout = 15000) {
    return new Promise((resolve, reject) => {
        const env = { ...process.env, PATH: `${os.homedir()}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin` };
        exec(`"${CAM_PATH}" ${args}`, { timeout, env }, (err, stdout, stderr) => {
            if (err && err.killed) return reject(new Error('命令超时'));
            resolve({ stdout: stdout.trim(), stderr: stderr.trim(), exitCode: err ? err.code : 0 });
        });
    });
}

function parseBody(req) {
    return new Promise((resolve) => {
        let body = '';
        req.on('data', c => body += c);
        req.on('end', () => {
            try { resolve(JSON.parse(body)); } catch { resolve({}); }
        });
    });
}

function checkAuth(req) {
    const auth = req.headers['authorization'] || '';
    return auth === `Bearer ${AUTH_TOKEN}`;
}

function sendJson(res, code, data) {
    res.writeHead(code, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(data));
}

const server = http.createServer(async (req, res) => {
    // CORS
    res.setHeader('Access-Control-Allow-Origin', '*');
    if (req.method === 'OPTIONS') { res.writeHead(200); res.end(); return; }

    // Health check
    if (req.url === '/health') {
        return sendJson(res, 200, { status: 'ok', cam: CAM_PATH });
    }

    // Auth check
    if (!checkAuth(req)) {
        return sendJson(res, 401, { error: '认证失败' });
    }

    try {
        // GET /cam/list - 列出运行中的 agent
        if (req.method === 'GET' && req.url === '/cam/list') {
            const result = await runCam('list');
            return sendJson(res, 200, { output: result.stdout });
        }

        // GET /cam/sessions - 列出会话
        if (req.method === 'GET' && req.url === '/cam/sessions') {
            const result = await runCam('sessions');
            return sendJson(res, 200, { output: result.stdout });
        }

        // GET /cam/service-status - 服务状态
        if (req.method === 'GET' && req.url === '/cam/service-status') {
            const result = await runCam('service status');
            return sendJson(res, 200, { output: result.stdout });
        }

        // POST /cam/start - 启动 Claude agent
        if (req.method === 'POST' && req.url === '/cam/start') {
            const body = await parseBody(req);
            const prompt = body.prompt || '';
            const cwd = body.cwd || os.homedir();
            const agent = body.agent || 'claude-code';
            let args = `start --agent ${agent} --cwd "${cwd}"`;
            if (prompt) args += ` "${prompt}"`;
            const result = await runCam(args, 30000);
            return sendJson(res, 200, { output: result.stdout, stderr: result.stderr });
        }

        // POST /cam/kill - 终止 agent
        if (req.method === 'POST' && req.url === '/cam/kill') {
            const body = await parseBody(req);
            if (!body.pid) return sendJson(res, 400, { error: '需要 pid 参数' });
            const result = await runCam(`kill ${body.pid}`);
            return sendJson(res, 200, { output: result.stdout });
        }

        // POST /cam/reply - 回复 agent
        if (req.method === 'POST' && req.url === '/cam/reply') {
            const body = await parseBody(req);
            if (!body.response) return sendJson(res, 400, { error: '需要 response 参数' });
            let args = `reply "${body.response}"`;
            if (body.all) args += ' --all';
            if (body.risk) args += ` --risk ${body.risk}`;
            const result = await runCam(args);
            return sendJson(res, 200, { output: result.stdout });
        }

        // GET /cam/pending - 待确认的请求
        if (req.method === 'GET' && req.url === '/cam/pending') {
            const result = await runCam('pending-confirmations');
            return sendJson(res, 200, { output: result.stdout });
        }

        // POST /cam/exec - 执行任意 cam 命令（高级）
        if (req.method === 'POST' && req.url === '/cam/exec') {
            const body = await parseBody(req);
            if (!body.command) return sendJson(res, 400, { error: '需要 command 参数' });
            // 安全检查：只允许 cam 命令
            const result = await runCam(body.command);
            return sendJson(res, 200, { output: result.stdout, stderr: result.stderr, exitCode: result.exitCode });
        }

        sendJson(res, 404, { error: 'Not found' });
    } catch (err) {
        sendJson(res, 500, { error: err.message });
    }
});

server.listen(PORT, '0.0.0.0', () => {
    console.log(`🔌 CAM API 服务已启动`);
    console.log(`   地址: http://0.0.0.0:${PORT}`);
    console.log(`   Token: ${AUTH_TOKEN}`);
    console.log(`   CAM: ${CAM_PATH}`);
});
