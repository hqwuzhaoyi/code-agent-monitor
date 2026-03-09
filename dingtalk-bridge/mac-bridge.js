#!/usr/bin/env node
/**
 * Mac Bridge Client — 反向通道桥接
 * 通过 WebSocket 连接 NAS 上的 dingtalk-ai，接收并执行命令
 * 支持: 直接命令执行 + tmux 会话管理 + Claude Code 启动
 *
 * 用法: node mac-bridge.js [NAS地址]
 * 例:   node mac-bridge.js ws://100.99.221.113:3211/ws
 */
const WebSocket = require('ws');
const { exec, execSync, spawn } = require('child_process');
const os = require('os');
const path = require('path');
const net = require('net');
const http = require('http');

const LOCAL_PORT = parseInt(process.env.LOCAL_DASHBOARD_PORT || '3456');

// ─── 配置 ───────────────────────────────────────────────────────────
// 连接目标：按优先级排列
const TAILSCALE_URL = process.env.BRIDGE_TAILSCALE || 'ws://100.99.221.113:3211/ws';
const CLOUDFLARE_URL = process.env.BRIDGE_CLOUDFLARE || 'wss://openclaw.jcylite.dpdns.org/ws';
// 手动覆盖（命令行参数或环境变量）
const SERVER_URL_OVERRIDE = process.argv[2] || process.env.BRIDGE_SERVER || null;

const WS_TOKEN = process.env.WS_TOKEN || 'dingtalk-bridge-2026';
const HEARTBEAT_INTERVAL = 15_000;
const RECONNECT_BASE = 3_000;
const RECONNECT_MAX = 60_000;
const COMMAND_TIMEOUT = 30_000;

const CAM_STATE_INTERVAL = 5_000;
const SNAPSHOT_LINES = 200;
const SNAPSHOT_INTERVAL = 15_000; // 降级模式下终端快照推送间隔

let activeServerUrl = null;    // 实际连接的 URL
let lastSuccessUrl = null;     // 上次成功连接的 URL（重连时优先复用）
let isDegradedMode = false;    // 是否降级模式（CF Tunnel）
let consecutiveFailures = 0;   // 连续失败次数（用于决定是否重新探测）

let reconnectDelay = RECONNECT_BASE;
let ws = null;
let heartbeatTimer = null;
let camStateTimer = null;
let isShuttingDown = false;

// ─── 设备信息采集 ───────────────────────────────────────────────────
function getDeviceInfo() {
  const hostname = os.hostname().replace(/\.local$/, '');
  const ifaces = os.networkInterfaces();
  let ip = '';
  for (const name of Object.keys(ifaces)) {
    for (const iface of ifaces[name]) {
      if (iface.family === 'IPv4' && !iface.internal && !iface.address.startsWith('100.')) {
        if (!ip) ip = iface.address;
      }
    }
  }

  const totalMem = os.totalmem();
  const freeMem = os.freemem();
  const usedMem = totalMem - freeMem;

  return {
    id: hostname.toLowerCase().replace(/\s+/g, '-'),
    name: hostname,
    type: 'Mac',
    ip,
    hostname: os.hostname(),
    os: `macOS ${process.env.MACOS_VERSION || ''}`.trim(),
    cpu: `${os.cpus()[0]?.model || ''} (${os.cpus().length} cores)`,
    memory: `${(usedMem / 1073741824).toFixed(1)}G/${(totalMem / 1073741824).toFixed(1)}G`,
    uptime: formatUptime(os.uptime()),
    capabilities: ['exec', 'tmux', 'claude-code'],
  };
}

function formatUptime(seconds) {
  if (seconds < 3600) return `${Math.floor(seconds / 60)}分钟`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}小时`;
  return `${Math.floor(seconds / 86400)}天${Math.floor((seconds % 86400) / 3600)}小时`;
}

// ─── 命令执行 ───────────────────────────────────────────────────────
function executeCommand(command, timeout = COMMAND_TIMEOUT) {
  return new Promise((resolve) => {
    const env = {
      ...process.env,
      PATH: `${os.homedir()}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin`,
    };

    exec(command, { timeout, env, cwd: os.homedir(), maxBuffer: 1024 * 1024 }, (err, stdout, stderr) => {
      if (err && err.killed) {
        resolve({ stdout: '', stderr: `命令超时 (${timeout / 1000}秒)`, exitCode: -1 });
        return;
      }
      resolve({
        stdout: stdout || '',
        stderr: stderr || '',
        exitCode: err ? (err.code || 1) : 0,
      });
    });
  });
}

// ─── tmux 操作 ──────────────────────────────────────────────────────

function tmuxExec(args) {
  try {
    const result = execSync(`tmux ${args}`, {
      encoding: 'utf-8',
      timeout: 10000,
      env: {
        ...process.env,
        PATH: `${os.homedir()}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin`,
      },
    });
    return { ok: true, output: result.trim() };
  } catch (err) {
    return { ok: false, error: err.stderr?.trim() || err.message };
  }
}

// 列出 tmux 会话
function tmuxList() {
  const result = tmuxExec('list-sessions -F "#{session_name}|#{session_created}|#{session_windows}|#{session_attached}"');
  if (!result.ok) {
    if (result.error.includes('no server running') || result.error.includes('no sessions')) {
      return { sessions: [] };
    }
    return { error: result.error };
  }

  const sessions = result.output.split('\n').filter(Boolean).map(line => {
    const [name, created, windows, attached] = line.split('|');
    return {
      name,
      created: new Date(parseInt(created) * 1000).toISOString(),
      windows: parseInt(windows),
      attached: attached === '1',
    };
  });
  return { sessions };
}

// 在指定目录启动 Claude Code（通过 cam start，确保注册到 agents.json）
function buildCamStartCommand(cwd, name) {
  return `cam start --cwd "${cwd}" --name ${name} --json`;
}

function parseCamStartResult(stdout, stderr, exitCode, cwd, name, prompt) {
  if (exitCode !== 0) {
    return { error: `cam start 失败: ${stderr || stdout}` };
  }

  let parsed = null;
  try {
    parsed = JSON.parse((stdout || '').trim());
  } catch {}

  const tmuxSession = parsed?.tmux_session || name;
  const agentId = parsed?.agent_id;

  return {
    status: 'started',
    agent_id: agentId,
    session: tmuxSession,
    tmux_session: tmuxSession,
    working_directory: parsed?.project_path || cwd,
    prompt: prompt || '',
    message: `Claude Code 已在 tmux 会话 "${tmuxSession}" 中启动，工作目录: ${parsed?.project_path || cwd}。${prompt ? 'Prompt 将在就绪后自动发送。' : ''}可以用 check_tmux_output 查看进度。`,
  };
}

async function tmuxStartClaude(workingDirectory, prompt, sessionName) {
  // 展开 ~ 路径
  const cwd = workingDirectory.replace(/^~/, os.homedir());

  // 生成会话名
  const name = sessionName || `claude-${Date.now().toString(36)}`;

  // 检查目录是否存在
  try {
    require('fs').accessSync(cwd);
  } catch {
    return { error: `目录不存在: ${cwd}` };
  }

  // 检查会话是否已存在
  const existing = tmuxExec(`has-session -t ${name}`);
  if (existing.ok) {
    return { error: `tmux 会话 "${name}" 已存在，请用其他名称或先关闭它` };
  }

  // 拆分为两步：先不带 prompt 启动（秒级返回），再异步发送 prompt
  // 因为 cam start 带 prompt 会同步等待 Claude Code 就绪（最多 30s），容易超时
  const camCmd = buildCamStartCommand(cwd, name);
  console.log(`🚀 执行: ${camCmd}`);
  const result = await executeCommand(camCmd, 15000);

  if (result.exitCode !== 0) {
    return { error: `cam start 失败: ${result.stderr || result.stdout}` };
  }

  const startResult = parseCamStartResult(
    result.stdout,
    result.stderr,
    result.exitCode,
    cwd,
    name,
    prompt
  );

  console.log(`🚀 Claude Code 已启动: session=${startResult.tmux_session}, cwd=${startResult.working_directory}`);

  // 异步发送 prompt（后台轮询等待就绪，不阻塞返回）
  if (prompt) {
    (async () => {
      const maxWait = 60; // 最多等 60 秒
      for (let i = 0; i < maxWait; i++) {
        await new Promise(r => setTimeout(r, 1000));
        try {
          const output = execSync(`tmux capture-pane -t ${startResult.tmux_session} -p -l 20`, { encoding: 'utf-8', timeout: 5000 });
          // Claude Code 就绪标志：出现 ">" 提示符或 "What would you like to do?" 等
          if (/^[>❯\$]\s*$/m.test(output) || /claude/i.test(output)) {
            await new Promise(r => setTimeout(r, 1000)); // 额外等 1s 确保稳定
            tmuxExec(`send-keys -t ${startResult.tmux_session} -l ${JSON.stringify(prompt)}`);
            tmuxExec(`send-keys -t ${startResult.tmux_session} Enter`);
            console.log(`📝 Prompt 已发送到 ${startResult.tmux_session} (等待 ${i + 1}s)`);
            return;
          }
        } catch {}
      }
      console.warn(`⚠️ ${startResult.tmux_session} 未就绪，prompt 未发送（等待超时 ${maxWait}s）`);
    })().catch(err => console.error(`❌ 发送 prompt 失败: ${err.message}`));
  }

  return startResult;
}

// 向 tmux 会话发送文本
function tmuxSend(session, text, pressEnter = true) {
  // 检查会话是否存在
  const exists = tmuxExec(`has-session -t ${session}`);
  if (!exists.ok) {
    return { error: `tmux 会话 "${session}" 不存在` };
  }

  // 使用 -l 标志发送字面文本
  const sendResult = tmuxExec(`send-keys -t ${session} -l ${JSON.stringify(text)}`);
  if (!sendResult.ok) {
    return { error: `发送失败: ${sendResult.error}` };
  }

  // 按回车
  if (pressEnter) {
    tmuxExec(`send-keys -t ${session} Enter`);
  }

  return { status: 'sent', session, text, pressEnter };
}

// 捕获 tmux 会话输出
function tmuxCapture(session, lines = 50) {
  const exists = tmuxExec(`has-session -t ${session}`);
  if (!exists.ok) {
    return { error: `tmux 会话 "${session}" 不存在` };
  }

  // -J: join wrapped lines  -e: 保留 ANSI 颜色转义序列
  const result = tmuxExec(`capture-pane -t ${session} -p -e -J -S -${lines}`);
  if (!result.ok) {
    return { error: `捕获输出失败: ${result.error}` };
  }

  // 清理输出：去掉末尾空行
  let output = result.output.replace(/\n+$/, '');

  // 超长输出截断（保留尾部），返回 truncated 标志供前端按需加载更多
  const TERM_MAX_CHARS = 50000;
  const truncated = output.length > TERM_MAX_CHARS;
  if (truncated) {
    output = output.substring(output.length - TERM_MAX_CHARS);
  }

  return { session, output, lines, truncated };
}

// ─── CAM 状态推送 ────────────────────────────────────────────────────

// 内容变化检测：存储上一次快照，用于比较
const prevAgentSnapshots = new Map(); // agent_id → content string
// 防抖：连续稳定计数器（避免工具调用间歇被误判为 idle）
const stableCount5s = new Map();      // agent_id → 5秒检测连续稳定次数
const STABLE_THRESHOLD_5S = 2;        // 需要连续 2 次稳定（10秒）才确认 idle
const fastIdleConsecutive = new Map(); // agent_id → 1秒检测连续 idle 次数
const FAST_IDLE_THRESHOLD = 5;        // 需要连续 5 次无变化（5秒）才确认 idle

// 计算两次快照间的有意义变化行数（排除 prompt/状态栏/分隔线变化）
function countSignificantChanges(prev, curr) {
  const prevLines = prev.split('\n');
  const currLines = curr.split('\n');
  let changes = 0;
  const maxLen = Math.max(prevLines.length, currLines.length);
  for (let i = 0; i < maxLen; i++) {
    if ((prevLines[i] || '') !== (currLines[i] || '')) {
      const line = (currLines[i] || '').trim();
      if (/^❯/.test(line)) continue;              // prompt 行（用户打字）
      if (/[🤖⏵]/.test(line)) continue;           // 状态栏
      if (/bypass permissions|tokens|Update available|shift\+tab/i.test(line)) continue;
      if (/^[─━═]+$/.test(line)) continue;         // 分隔线
      changes++;
    }
  }
  return changes;
}

// 从 agents.json 读取 CAM 管理的 agent，比 `cam list`（进程扫描）精确
function readAgentsJson() {
  try {
    const fs = require('fs');
    const data = JSON.parse(fs.readFileSync(
      path.join(os.homedir(), '.config/code-agent-monitor/agents.json'), 'utf-8'
    ));
    const agents = data.agents || [];
    // 过滤已死亡的 tmux session，并检测 idle 状态
    return agents.filter(a => {
      if (!a.tmux_session) return false;
      const check = tmuxExec(`has-session -t ${a.tmux_session}`);
      return check.ok;
    }).map(a => {
      // 通过终端快照检测 idle/busy
      // Claude Code 终端结构（空闲时）：
      //   ✻ Churned for 1m 9s        ← 完成时态
      //   ──────────────────
      //   ❯                           ← 空提示符，夹在分隔线之间
      //   ──────────────────
      //   🤖 状态栏 + ⏵⏵ bypass + 碎片
      //
      // Claude Code 终端结构（工作中）：
      //   ❯ 用户发的消息
      //   ✽ Billowing… (32s)          ← 进行时 spinner
      //   ──────────────────
      //   ❯                           ← 空提示符（底部）
      //   ──────────────────
      //   🤖 状态栏 + 碎片
      try {
        // 增大捕获窗口：80→150行，提升 spinner 在长输出时的可见性
        const cap = tmuxExec(`capture-pane -t ${a.tmux_session} -p -S -150`);
        if (cap.ok) {
          const id = a.agent_id || a.tmux_session;
          const content = cap.output;
          const lines = content.split('\n');
          const isSpinner = (s) => /^[✻✽✳✺✹✸✷✶✵✴✲✱✰]/.test(s);
          const isSeparator = (s) => /^[─━═]+$/.test(s);
          const isDecoration = (s) => {
            if (!s) return true;
            if (isSeparator(s)) return true;
            if (/[🤖⏵]/.test(s)) return true;
            if (/bypass permissions|Update available|shift\+tab/i.test(s)) return true;
            return false;
          };

          // 步骤1：从底部跳过装饰行+碎片，找到底部的 ❯
          let bottomPromptIdx = -1;
          for (let i = lines.length - 1; i >= 0; i--) {
            const trimmed = lines[i].trim();
            if (!trimmed) continue;
            if (isDecoration(trimmed)) continue;
            // 窄窗口碎片：前导空格 > 实际文本
            const leading = lines[i].length - lines[i].trimStart().length;
            if (leading > 0 && leading > lines[i].trimStart().length) continue;
            // 到达实际内容行
            if (/^❯/.test(trimmed)) { bottomPromptIdx = i; }
            break;
          }

          if (bottomPromptIdx === -1) {
            // 没找到 ❯ → 全屏输出中（agent 正在大量输出）
            prevAgentSnapshots.set(id, content);
            stableCount5s.set(id, 0);
            console.log(`📊 [${id}] → busy (无❯，全屏输出中)`);
            return { ...a, status: 'busy' };
          }

          // 步骤2：从底部 ❯ 往上扫描找 spinner
          // 关键改进：不在内容行 break，穿透工具输出继续扫描
          // 仅在遇到用户 prompt（❯ 后有文字 = 当前任务起点）时停止
          let spinnerStatus = null;
          let spinnerLine = '';
          let scanStopReason = 'limit';
          for (let i = bottomPromptIdx - 1; i >= Math.max(0, bottomPromptIdx - 130); i--) {
            const trimmed = lines[i].trim();
            if (!trimmed) continue;
            if (isSeparator(trimmed)) continue;
            if (isDecoration(trimmed)) continue;
            if (isSpinner(trimmed)) {
              // 进行时：不含 "for 数字"（如 ✽ Billowing… (32s)）
              // 完成时：含 "for 数字"（如 ✻ Churned for 1m 9s）
              spinnerStatus = /\bfor \d/.test(trimmed) ? 'idle' : 'busy';
              spinnerLine = trimmed;
              scanStopReason = 'spinner';
              break;
            }
            // 遇到用户 prompt（❯ 后跟非空内容）→ 当前任务起点，停止扫描
            if (/^❯\s+\S/.test(trimmed)) {
              scanStopReason = 'user_prompt';
              break;
            }
            // 其他内容行（工具输出等）→ 继续扫描，不 break
          }

          if (spinnerStatus) {
            prevAgentSnapshots.set(id, content);
            stableCount5s.set(id, spinnerStatus === 'idle' ? STABLE_THRESHOLD_5S : 0);
            console.log(`📊 [${id}] → ${spinnerStatus} (spinner: "${spinnerLine.substring(0, 40)}")`);
            return { ...a, status: spinnerStatus };
          }

          // 步骤3：未找到 spinner → 用内容变化检测 + 防抖
          // 场景：spinner 已滚出视野，或非 Claude Code agent
          const prevContent = prevAgentSnapshots.get(id);
          prevAgentSnapshots.set(id, content);

          if (prevContent && prevContent !== content) {
            const changed = countSignificantChanges(prevContent, content);
            stableCount5s.set(id, 0); // 有变化 → 重置稳定计数
            if (changed > 0) {
              console.log(`📊 [${id}] → busy (内容变化 ${changed} 行, scan止于: ${scanStopReason})`);
              return { ...a, status: 'busy' };
            }
          }

          // 步骤4：内容稳定 → 防抖判断
          // 需要连续 N 次检测都稳定才确认 idle（避免工具间歇误判）
          const stable = (stableCount5s.get(id) || 0) + 1;
          stableCount5s.set(id, stable);

          if (stable < STABLE_THRESHOLD_5S) {
            console.log(`📊 [${id}] → busy (稳定计数 ${stable}/${STABLE_THRESHOLD_5S}, 等待确认, scan止于: ${scanStopReason})`);
            return { ...a, status: 'busy' };
          }

          console.log(`📊 [${id}] → idle (稳定 ${stable} 次确认, scan止于: ${scanStopReason})`);
          return { ...a, status: 'idle' };
        }
      } catch {}
      return { ...a, status: a.status || 'unknown' };
    });
  } catch {
    return [];
  }
}

const lastAgentStatuses = new Map();

// 快速 idle 检测器（1 秒间隔），独立于 5 秒的状态推送
const notifiedIdle = new Set(); // 防重复通知
const busySince = new Map(); // id → 首次检测到 busy 的时间戳
const SUSTAINED_BUSY_MS = 3_000; // 持续忙碌 3 秒后清除 idle 标记（允许短任务完成后再次触发）
let idleCheckWarmedUp = false; // 首次扫描只建立基线，不触发通知

const fastCheckSnapshots = new Map(); // agent_id → content (for 1-second delta)

function checkAgentIdleStatus() {
  try {
    const fs = require('fs');
    const data = JSON.parse(fs.readFileSync(
      path.join(os.homedir(), '.config/code-agent-monitor/agents.json'), 'utf-8'
    ));
    const agents = data.agents || [];
    for (const a of agents) {
      if (!a.tmux_session) continue;
      const id = a.agent_id || a.tmux_session;
      try {
        // 增大快速检测的捕获范围：5→30行，获得更可靠的内容变化信号
        const cap = tmuxExec(`capture-pane -t ${a.tmux_session} -p -S -30`);
        if (!cap.ok) continue;
        const content = cap.output;
        const prevContent = fastCheckSnapshots.get(id);
        fastCheckSnapshots.set(id, content);

        if (!prevContent) {
          // 首次检测，建立基线
          if (!idleCheckWarmedUp) {
            lastAgentStatuses.set(id, 'unknown');
          }
          continue;
        }

        // 用内容变化检测 busy/idle（替代不可靠的 ❯ 检测）
        // 排除 prompt/状态栏变化，只关注有意义的内容变更
        const meaningfulChanges = countSignificantChanges(prevContent, content);
        let status;

        if (meaningfulChanges > 0) {
          // 有变化 → busy，重置 idle 计数
          status = 'busy';
          fastIdleConsecutive.set(id, 0);
        } else {
          // 无变化 → 需要连续 N 次确认才算 idle（防抖）
          const idleCount = (fastIdleConsecutive.get(id) || 0) + 1;
          fastIdleConsecutive.set(id, idleCount);
          status = idleCount >= FAST_IDLE_THRESHOLD ? 'idle' : 'busy';
        }

        const oldStatus = lastAgentStatuses.get(id);
        if (status === 'busy') {
          if (!busySince.has(id)) {
            busySince.set(id, Date.now());
          }
          // 持续忙碌超过阈值后清除已通知标记（认为是新任务）
          if (Date.now() - busySince.get(id) > SUSTAINED_BUSY_MS) {
            notifiedIdle.delete(id);
          }
        } else {
          busySince.delete(id);
        }
        // 首次扫描（warmup）只建立基线，不触发通知
        if (!idleCheckWarmedUp) {
          lastAgentStatuses.set(id, status);
          continue;
        }
        if (oldStatus === 'busy' && status === 'idle' && !notifiedIdle.has(id)) {
          notifiedIdle.add(id);
          const idleCount = fastIdleConsecutive.get(id) || 0;
          console.log(`🔔 Agent 完成检测: ${id} (busy→idle, 连续稳定 ${idleCount} 次)`);
          if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ type: 'agent_completed', agent: { ...a, status }, timestamp: Date.now() }));
          }
          // 触发本地 SSE 即时状态更新
          computeCamState().then(s => broadcastLocalSSE({ type: 'state', macOnline: true, ...s })).catch(() => {});
        }
        lastAgentStatuses.set(id, status);
      } catch {}
    }
    if (!idleCheckWarmedUp) {
      idleCheckWarmedUp = true;
      console.log(`🔄 Idle 检测器已预热，基线: ${lastAgentStatuses.size} 个 agent`);
    }
  } catch {}
}

// 项目目录缓存（每 30 秒刷新一次，不阻塞每次推送）
let _projectDirs = [];
let _projectDirsLastRefresh = 0;
const PROJECT_DIRS_INTERVAL = 30_000;

function refreshProjectDirs() {
  try {
    const projectRoot = path.join(os.homedir(), 'project');
    const entries = require('fs').readdirSync(projectRoot, { withFileTypes: true });
    _projectDirs = entries
      .filter(e => e.isDirectory() && !e.name.startsWith('.'))
      .map(e => e.name);
    _projectDirsLastRefresh = Date.now();
  } catch { /* ~/project 不存在也没关系 */ }
}

async function computeCamState() {
  const agents = readAgentsJson();
  let pending = [];
  try {
    const result = await executeCommand('cam pending-confirmations --json', 5000);
    if (result.exitCode === 0 && result.stdout.trim()) {
      pending = JSON.parse(result.stdout);
    }
  } catch {}
  if (Date.now() - _projectDirsLastRefresh > PROJECT_DIRS_INTERVAL) {
    refreshProjectDirs();
  }
  return { agents, pending, projectDirs: _projectDirs, timestamp: Date.now() };
}

async function pushCamState() {
  try {
    const state = await computeCamState();
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'cam_state_push', ...state }));
    }
    broadcastLocalSSE({ type: 'state', macOnline: true, ...state });
  } catch (err) {
    console.error(`❌ CAM 状态推送失败: ${err.message}`);
  }
}

// ─── 本地 SSE 管理 ──────────────────────────────────────────────────
const localSSEClients = new Set();

function broadcastLocalSSE(data) {
  if (localSSEClients.size === 0) return;
  const msg = `data: ${JSON.stringify(data)}\n\n`;
  for (const res of localSSEClients) {
    try { res.write(msg); } catch { localSSEClients.delete(res); }
  }
}

// ─── Local Dashboard Server ─────────────────────────────────────────

function parseBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on('data', c => {
      size += c.length;
      if (size > 10485760) { req.destroy(); reject(new Error('body too large')); return; }
      chunks.push(c);
    });
    req.on('end', () => {
      try { resolve(JSON.parse(Buffer.concat(chunks).toString())); }
      catch (e) { reject(e); }
    });
    req.on('error', reject);
  });
}

function jsonRes(res, code, obj) {
  res.writeHead(code, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(obj));
}

let localServer;
const localHandler = async (req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
  if (req.method === 'OPTIONS') { res.writeHead(204); res.end(); return; }

  const url = new URL(req.url, `http://localhost:${LOCAL_PORT}`);
  const pathname = url.pathname;

  try {
    // ─── GET / ── Dashboard HTML ───
    if (req.method === 'GET' && (pathname === '/' || pathname === '/dashboard')) {
      try {
        const fs = require('fs');
        const html = fs.readFileSync(path.join(__dirname, 'dashboard.html'), 'utf-8');
        res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
        res.end(html);
      } catch (err) {
        jsonRes(res, 500, { ok: false, error: `dashboard.html 读取失败: ${err.message}` });
      }
      return;
    }

    // ─── GET /api/cam/state ── 健康检查 / 认证网关 ───
    if (req.method === 'GET' && pathname === '/api/cam/state') {
      jsonRes(res, 200, { ok: true });
      return;
    }

    // ─── GET /api/cam/events ── SSE 流 ───
    if (req.method === 'GET' && pathname === '/api/cam/events') {
      res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
      });
      // 立即推送当前状态
      computeCamState().then(state => {
        const msg = `data: ${JSON.stringify({ type: 'state', macOnline: true, ...state })}\n\n`;
        try { res.write(msg); } catch {}
      }).catch(() => {});
      localSSEClients.add(res);
      req.on('close', () => localSSEClients.delete(res));
      return;
    }

    // ─── GET /api/cam/terminal ── 终端捕获 ───
    if (req.method === 'GET' && pathname === '/api/cam/terminal') {
      const session = url.searchParams.get('session');
      const lines = parseInt(url.searchParams.get('lines') || '200');
      if (!session) { jsonRes(res, 400, { ok: false, error: 'session required' }); return; }
      const result = tmuxCapture(session, lines);
      if (result.error) {
        jsonRes(res, 200, { ok: false, error: result.error });
      } else {
        jsonRes(res, 200, { ok: true, output: result.output });
      }
      return;
    }

    // ─── POST /api/cam/login ── 本地免认证 ───
    if (req.method === 'POST' && pathname === '/api/cam/login') {
      jsonRes(res, 200, { ok: true });
      return;
    }

    // ─── POST /api/cam/send ── 发送文本到 tmux ───
    if (req.method === 'POST' && pathname === '/api/cam/send') {
      const body = await parseBody(req);
      const { session, text, pressEnter } = body;
      if (!session || text == null) { jsonRes(res, 400, { ok: false, error: 'session and text required' }); return; }
      const result = tmuxSend(session, text, pressEnter !== false);
      if (result.error) {
        jsonRes(res, 200, { ok: false, error: result.error });
      } else {
        jsonRes(res, 200, { ok: true, result });
      }
      return;
    }

    // ─── POST /api/cam/send-key ── 发送原始按键 ───
    if (req.method === 'POST' && pathname === '/api/cam/send-key') {
      const body = await parseBody(req);
      const { session, key } = body;
      if (!session || !key) { jsonRes(res, 400, { ok: false, error: 'session and key required' }); return; }
      const allowed = ['Escape', 'C-c', 'C-d', 'C-z', 'Enter', 'Up', 'Down'];
      if (!allowed.includes(key)) { jsonRes(res, 400, { ok: false, error: `不允许的按键: ${key}` }); return; }
      const exists = tmuxExec(`has-session -t ${session}`);
      if (!exists.ok) { jsonRes(res, 404, { ok: false, error: `会话 "${session}" 不存在` }); return; }
      const result = tmuxExec(`send-keys -t ${session} ${key}`);
      jsonRes(res, 200, { ok: result.ok, result: { status: 'sent', key } });
      return;
    }

    // ─── POST /api/cam/reply ── CAM 回复 ───
    if (req.method === 'POST' && pathname === '/api/cam/reply') {
      const body = await parseBody(req);
      const { agentId, input } = body;
      if (!agentId || input == null) { jsonRes(res, 400, { ok: false, error: 'agentId and input required' }); return; }
      const camCmd = `cam reply "${input.replace(/"/g, '\\"')}" --agent "${agentId}"`;
      const result = await executeCommand(camCmd, 10000);
      jsonRes(res, 200, { ok: result.exitCode === 0, result });
      return;
    }

    // ─── POST /api/cam/agent/start ── 启动 Agent ───
    if (req.method === 'POST' && pathname === '/api/cam/agent/start') {
      const body = await parseBody(req);
      const { cwd, prompt, name } = body;
      if (!cwd) { jsonRes(res, 400, { ok: false, error: 'cwd required' }); return; }
      const result = await tmuxStartClaude(cwd, prompt || '', name || '');
      if (result.error) {
        jsonRes(res, 500, { ok: false, error: result.error });
      } else {
        jsonRes(res, 200, {
          ok: true, accepted: true,
          device: os.hostname(),
          agent: { agent_id: result.agent_id, tmux_session: result.tmux_session || result.session },
          message: result.message || '已启动',
        });
      }
      return;
    }

    // ─── POST /api/cam/agent/delete ── 删除 Agent ───
    if (req.method === 'POST' && pathname === '/api/cam/agent/delete') {
      const body = await parseBody(req);
      const { agentId, killSession } = body;
      if (!agentId) { jsonRes(res, 400, { ok: false, error: 'agentId required' }); return; }
      const safeId = agentId.replace(/[^a-zA-Z0-9_-]/g, '');
      if (killSession) {
        try { tmuxExec(`kill-session -t ${safeId}`); } catch {}
      }
      const pyCmd = `python3 -c "
import json, os
p = os.path.expanduser('~/.config/code-agent-monitor/agents.json')
d = json.load(open(p))
d['agents'] = [a for a in d['agents'] if a.get('agent_id') != '${safeId}' and a.get('tmux_session') != '${safeId}']
json.dump(d, open(p, 'w'), indent=2)
print('removed')
"`;
      await executeCommand(pyCmd, 10000);
      jsonRes(res, 200, { ok: true, deleted: agentId });
      return;
    }

    // ─── POST /api/cam/agent/rename ── 重命名 Agent ───
    if (req.method === 'POST' && pathname === '/api/cam/agent/rename') {
      const body = await parseBody(req);
      const { agentId, newName } = body;
      if (!agentId || !newName) { jsonRes(res, 400, { ok: false, error: 'agentId and newName required' }); return; }
      const safeName = newName.replace(/[^a-zA-Z0-9_\-\u4e00-\u9fff]/g, '').substring(0, 40);
      if (!safeName) { jsonRes(res, 400, { ok: false, error: '名称无效' }); return; }
      const safeId = agentId.replace(/'/g, '');
      const pyCmd = `python3 -c "
import json, os
p = os.path.expanduser('~/.config/code-agent-monitor/agents.json')
d = json.load(open(p))
found = False
for a in d['agents']:
    if a.get('agent_id') == '${safeId}' or a.get('tmux_session') == '${safeId}':
        a['agent_id'] = '${safeName.replace(/'/g, '')}'
        found = True
        break
if found:
    json.dump(d, open(p, 'w'), indent=2)
    print('renamed')
else:
    print('not_found')
"`;
      const result = await executeCommand(pyCmd, 10000);
      const output = (result.stdout || '').trim();
      if (output.includes('not_found')) {
        jsonRes(res, 404, { ok: false, error: `Agent "${agentId}" 未找到` });
      } else {
        jsonRes(res, 200, { ok: true, oldName: agentId, newName: safeName });
      }
      return;
    }

    // ─── POST /api/cam/agent/clone ── Fork Agent ───
    if (req.method === 'POST' && pathname === '/api/cam/agent/clone') {
      const body = await parseBody(req);
      const { agentId, name } = body;
      if (!agentId) { jsonRes(res, 400, { ok: false, error: 'agentId required' }); return; }
      const safeId = agentId.replace(/[^a-zA-Z0-9_\-\u4e00-\u9fff]/g, '');
      const nameFlag = name ? ` --name ${name.replace(/[^a-zA-Z0-9_-]/g, '').substring(0, 40)}` : '';
      const camCmd = `cam fork "${safeId}"${nameFlag} --json`;
      const result = await executeCommand(camCmd, 30000);
      const output = (result.stdout || '').trim();
      const stderr = (result.stderr || '').trim();
      try {
        const data = JSON.parse(output);
        jsonRes(res, 200, { ok: true, ...data });
      } catch {
        const combined = output || stderr;
        if (combined.includes('new_agent_id') || combined.includes('分叉成功')) {
          jsonRes(res, 200, { ok: true, sessionName: name || 'fork', raw: combined });
        } else {
          jsonRes(res, 500, { ok: false, error: combined || 'fork 命令无输出', exitCode: result.exitCode });
        }
      }
      return;
    }

    // ─── POST /api/cam/agent/restart ── 重启 Agent ───
    if (req.method === 'POST' && pathname === '/api/cam/agent/restart') {
      const body = await parseBody(req);
      const { session } = body;
      if (!session) { jsonRes(res, 400, { ok: false, error: 'session required' }); return; }
      const safeSession = session.replace(/[^a-zA-Z0-9_\-]/g, '');
      tmuxExec(`send-keys -t ${safeSession} C-c`);
      await new Promise(r => setTimeout(r, 800));
      tmuxSend(safeSession, '/exit');
      await new Promise(r => setTimeout(r, 3000));
      tmuxSend(safeSession, 'claude --dangerously-skip-permissions --continue');
      jsonRes(res, 200, { ok: true, session: safeSession });
      return;
    }

    // ─── POST /api/cam/upload-image ── 图片上传 ───
    if (req.method === 'POST' && pathname === '/api/cam/upload-image') {
      const body = await parseBody(req);
      const { fileName, data } = body;
      if (!fileName || !data) { jsonRes(res, 400, { ok: false, error: 'fileName and data required' }); return; }
      const fs = require('fs');
      const uploadDir = '/tmp/cam-uploads';
      if (!fs.existsSync(uploadDir)) fs.mkdirSync(uploadDir, { recursive: true });
      const safeName = fileName.replace(/[^a-zA-Z0-9._-]/g, '_').substring(0, 100);
      const resolved = path.resolve(uploadDir, safeName);
      if (!resolved.startsWith(uploadDir)) { jsonRes(res, 400, { ok: false, error: '路径不安全' }); return; }
      const buf = Buffer.from(data, 'base64');
      fs.writeFileSync(resolved, buf);
      jsonRes(res, 200, { ok: true, path: resolved, size: buf.length });
      return;
    }

    // ─── POST /api/cam/open-in-vscode ── 用 VS Code 打开目录 ───
    if (req.method === 'POST' && pathname === '/api/cam/open-in-vscode') {
      const body = await parseBody(req);
      const targetPath = typeof body.path === 'string' ? body.path.trim() : '';
      if (!targetPath) { jsonRes(res, 400, { ok: false, error: 'path required' }); return; }
      const resolvedPath = path.resolve(targetPath.replace(/^~(?=\/|$)/, os.homedir()));
      const result = await executeCommand(`code ${JSON.stringify(resolvedPath)}`, 10000);
      if (result.exitCode !== 0) {
        jsonRes(res, 500, { ok: false, error: result.stderr || result.stdout || 'code 命令执行失败' });
      } else {
        jsonRes(res, 200, { ok: true, path: resolvedPath });
      }
      return;
    }

    // ─── NAS-only stubs（本地模式不支持） ───
    if (req.method === 'GET' && pathname === '/api/cam/chat-history') {
      jsonRes(res, 200, { ok: true, conversations: {} });
      return;
    }
    if (req.method === 'POST' && pathname === '/api/cam/chat-history/clear') {
      jsonRes(res, 200, { ok: true });
      return;
    }
    if (req.method === 'GET' && pathname === '/api/cam/knowledge') {
      jsonRes(res, 200, { ok: true, files: [] });
      return;
    }
    if (pathname.startsWith('/api/cam/knowledge/')) {
      jsonRes(res, 200, { ok: false, error: '本地模式不支持知识库' });
      return;
    }

    // ─── 404 ───
    jsonRes(res, 404, { ok: false, error: 'Not found' });
  } catch (err) {
    jsonRes(res, 500, { ok: false, error: err.message });
  }
};

function startLocalServer() {
  localServer = http.createServer(localHandler);
  localServer.listen(LOCAL_PORT, '127.0.0.1', () => {
    console.log(`🌐 Local dashboard: http://localhost:${LOCAL_PORT}`);
  });
  localServer.on('error', (err) => {
    if (err.code === 'EADDRINUSE') {
      console.error(`❌ 端口 ${LOCAL_PORT} 被占用，本地 Dashboard 已禁用`);
    } else {
      console.error(`❌ 本地服务器错误: ${err.message}`);
    }
  });
  // 确保定时器不依赖 WS 连接
  if (!camStateTimer) {
    refreshProjectDirs();
    pushCamState();
    camStateTimer = setInterval(pushCamState, CAM_STATE_INTERVAL);
  }
  if (!global._idleCheckTimer) {
    global._idleCheckTimer = setInterval(checkAgentIdleStatus, 1000);
  }
}

// ─── Tailscale 探测 ─────────────────────────────────────────────────
function probeHost(host, port, timeoutMs = 3000) {
  return new Promise((resolve) => {
    const socket = new net.Socket();
    const timer = setTimeout(() => { socket.destroy(); resolve(false); }, timeoutMs);
    socket.once('connect', () => { clearTimeout(timer); socket.destroy(); resolve(true); });
    socket.once('error', () => { clearTimeout(timer); resolve(false); });
    socket.connect(port, host);
  });
}

// ─── 降级模式：终端快照推送 ──────────────────────────────────────────
function pushTerminalSnapshots() {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  try {
    const fs = require('fs');
    const data = JSON.parse(fs.readFileSync(
      path.join(os.homedir(), '.config/code-agent-monitor/agents.json'), 'utf-8'
    ));
    const agents = data.agents || [];
    const snapshots = {};

    for (const a of agents) {
      if (!a.tmux_session) continue;
      const cap = tmuxExec(`capture-pane -t ${a.tmux_session} -p -S -${SNAPSHOT_LINES}`);
      if (cap.ok) {
        snapshots[a.tmux_session] = cap.output;
      }
    }

    if (Object.keys(snapshots).length > 0) {
      ws.send(JSON.stringify({
        type: 'terminal_snapshots',
        snapshots,
        timestamp: Date.now(),
      }));
    }
  } catch (err) {
    console.error(`❌ 终端快照推送失败: ${err.message}`);
  }
}

// ─── WebSocket 连接 ─────────────────────────────────────────────────
async function connect() {
  if (isShuttingDown) return;

  // 决定连接目标
  if (SERVER_URL_OVERRIDE) {
    activeServerUrl = SERVER_URL_OVERRIDE;
    isDegradedMode = !activeServerUrl.includes('100.99.221.113');
    console.log(`📌 使用手动指定地址: ${activeServerUrl}`);
  } else if (lastSuccessUrl && consecutiveFailures < 3) {
    // 重连优先复用上次成功的地址，跳过探测（省 3 秒）
    activeServerUrl = lastSuccessUrl;
    isDegradedMode = !activeServerUrl.includes('100.99.221.113');
    console.log(`🔄 快速重连: ${activeServerUrl} (跳过探测)`);
  } else {
    // 首次连接或连续失败 3 次，重新探测
    if (consecutiveFailures >= 3) {
      console.log('🔍 连续失败 3 次，重新探测最佳路由...');
    }
    const tailscaleOk = await probeHost('100.99.221.113', 3211, 3000);
    if (tailscaleOk) {
      activeServerUrl = TAILSCALE_URL;
      isDegradedMode = false;
      console.log('🚀 Tailscale 可达，使用直连模式');
    } else {
      activeServerUrl = CLOUDFLARE_URL;
      isDegradedMode = true;
      console.log('⚠️ Tailscale 不可达，降级到 Cloudflare Tunnel');
    }
  }

  console.log(`🔗 连接 ${activeServerUrl} (${isDegradedMode ? '降级' : '直连'}) ...`);
  ws = new WebSocket(activeServerUrl);

  ws.on('open', () => {
    console.log('✅ WebSocket 已连接');
    lastSuccessUrl = activeServerUrl;
    consecutiveFailures = 0;
    reconnectDelay = RECONNECT_BASE;

    const deviceInfo = getDeviceInfo();
    ws.send(JSON.stringify({
      type: 'register',
      token: WS_TOKEN,
      device: deviceInfo,
      degraded: isDegradedMode,
    }));

    clearInterval(heartbeatTimer);
    heartbeatTimer = setInterval(() => {
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({
          type: 'heartbeat',
          device: getDeviceInfo(),
        }));
      }
    }, HEARTBEAT_INTERVAL);

    // CAM 状态推送（每 5 秒）
    clearInterval(camStateTimer);
    pushCamState(); // 立即推送一次
    camStateTimer = setInterval(pushCamState, CAM_STATE_INTERVAL);

    // Agent idle 检测（每 1 秒，快速捕捉 busy→idle 变化）
    if (global._idleCheckTimer) clearInterval(global._idleCheckTimer);
    global._idleCheckTimer = setInterval(checkAgentIdleStatus, 1000);

    // 降级模式：每 15 秒推送终端快照到 NAS 缓存
    clearInterval(global._termSnapshotTimer);
    if (isDegradedMode) {
      console.log('📸 降级模式：启动终端快照推送（15 秒间隔）');
      pushTerminalSnapshots(); // 立即推送一次
      global._termSnapshotTimer = setInterval(pushTerminalSnapshots, SNAPSHOT_INTERVAL);
    }
  });

  ws.on('message', async (data) => {
    let msg;
    try { msg = JSON.parse(data); } catch { return; }

    if (msg.type === 'registered') {
      console.log(`✅ 设备已注册: ${msg.deviceId}`);
      return;
    }

    if (msg.type === 'pong') return;

    if (msg.type === 'error') {
      console.error(`❌ 服务器错误: ${msg.message}`);
      return;
    }

    // ─── 直接命令执行 ───
    if (msg.type === 'exec') {
      const { id, command, timeout } = msg;
      console.log(`🔧 执行命令: ${command}`);
      const result = await executeCommand(command, timeout || COMMAND_TIMEOUT);
      console.log(`   → 退出码: ${result.exitCode}, 输出: ${result.stdout.length} 字符`);

      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({
          type: 'exec_result',
          id,
          stdout: result.stdout,
          stderr: result.stderr,
          exitCode: result.exitCode,
        }));
      }
      return;
    }

    // ─── tmux: 启动 Claude Code ───
    if (msg.type === 'tmux_start_claude') {
      const { id, working_directory, prompt, session_name } = msg;
      console.log(`🚀 启动 Claude Code: dir=${working_directory}, prompt=${prompt.substring(0, 50)}...`);
      const result = await tmuxStartClaude(working_directory, prompt, session_name);
      console.log(`   → ${result.status || 'error'}: ${result.session || result.error}`);

      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'tmux_result', id, data: result }));
      }
      return;
    }

    // ─── tmux: 发送文本 ───
    if (msg.type === 'tmux_send') {
      const { id, session, text, press_enter } = msg;
      console.log(`📝 tmux send: session=${session}, text=${text.substring(0, 50)}`);
      const result = tmuxSend(session, text, press_enter);

      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'tmux_result', id, data: result }));
      }
      return;
    }

    // ─── tmux: 发送原始按键（Escape、C-c 等） ───
    if (msg.type === 'tmux_send_key') {
      const { id, session, key } = msg;
      console.log(`⌨️ tmux send-key: session=${session}, key=${key}`);
      const exists = tmuxExec(`has-session -t ${session}`);
      if (!exists.ok) {
        if (ws && ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'tmux_result', id, data: { error: `会话 "${session}" 不存在` } }));
        }
        return;
      }
      const result = tmuxExec(`send-keys -t ${session} ${key}`);
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'tmux_result', id, data: { status: 'sent', key, ok: result.ok } }));
      }
      return;
    }

    // ─── 文件写入（用于图片上传） ───
    if (msg.type === 'file_write') {
      const { id, path: filePath, data, encoding } = msg;
      console.log(`📁 file write: ${filePath} (${(data || '').length} chars)`);
      try {
        const fs = require('fs');
        const path = require('path');
        // 安全检查：只允许写到 /tmp/cam-uploads/
        const uploadDir = '/tmp/cam-uploads';
        if (!fs.existsSync(uploadDir)) fs.mkdirSync(uploadDir, { recursive: true });
        const resolved = path.resolve(uploadDir, path.basename(filePath));
        if (!resolved.startsWith(uploadDir)) {
          throw new Error('路径不安全');
        }
        const buf = Buffer.from(data, encoding || 'base64');
        fs.writeFileSync(resolved, buf);
        console.log(`✅ 文件已写入: ${resolved} (${buf.length} bytes)`);
        if (ws && ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'tmux_result', id, data: { status: 'ok', path: resolved, size: buf.length } }));
        }
      } catch (err) {
        console.error(`❌ 文件写入失败: ${err.message}`);
        if (ws && ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'tmux_result', id, data: { error: err.message } }));
        }
      }
      return;
    }

    // ─── tmux: 捕获输出 ───
    if (msg.type === 'tmux_capture') {
      const { id, session, lines } = msg;
      console.log(`👀 tmux capture: session=${session}, lines=${lines}`);
      const result = tmuxCapture(session, lines);

      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'tmux_result', id, data: result }));
      }
      return;
    }

    // ─── tmux: 列出会话 ───
    if (msg.type === 'tmux_list') {
      const { id } = msg;
      console.log('📋 tmux list sessions');
      const result = tmuxList();

      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'tmux_result', id, data: result }));
      }
      return;
    }

    // ─── CAM 回复转发 ───
    if (msg.type === 'cam_reply') {
      const { id, agentId, input } = msg;
      console.log(`🔁 CAM 回复: agent=${agentId}, input=${input}`);
      const camCmd = `cam reply "${input.replace(/"/g, '\\"')}" --agent "${agentId}"`;
      const result = await executeCommand(camCmd, 10000);
      console.log(`   → 退出码: ${result.exitCode}`);

      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'exec_result', id, ...result }));
      }
      return;
    }
  });

  ws.on('close', () => {
    clearInterval(heartbeatTimer);
    clearInterval(camStateTimer);
    clearInterval(global._termSnapshotTimer);
    if (isShuttingDown) return;
    consecutiveFailures++;
    // 前 3 次快速重连（1s），之后指数退避
    const delay = consecutiveFailures <= 3 ? 1000 : reconnectDelay;
    console.log(`⚡ WebSocket 断开 (第${consecutiveFailures}次)，${delay / 1000}秒后重连...`);
    setTimeout(connect, delay);
    if (consecutiveFailures > 3) {
      reconnectDelay = Math.min(reconnectDelay * 1.5, RECONNECT_MAX);
    }
  });

  ws.on('error', (err) => {
    if (err.code === 'ECONNREFUSED') {
      console.error(`❌ 连接被拒绝: ${SERVER_URL}`);
    } else {
      console.error(`❌ WebSocket 错误: ${err.message}`);
    }
  });
}

// ─── 优雅退出 ───────────────────────────────────────────────────────
function shutdown() {
  if (isShuttingDown) return;
  isShuttingDown = true;
  console.log('\n👋 正在断开连接...');
  clearInterval(heartbeatTimer);
  clearInterval(camStateTimer);
  clearInterval(global._termSnapshotTimer);
  if (localServer) localServer.close();
  for (const r of localSSEClients) { try { r.end(); } catch {} }
  localSSEClients.clear();
  if (ws) ws.close();
  process.exit(0);
}

process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);

// ─── 启动 ───────────────────────────────────────────────────────────
console.log('');
console.log('🖥️  ═══════════════════════════════════════════════');
console.log('   Mac Bridge Client + Claude Code');
console.log('   ─────────────────────────────────────────────');
console.log(`   直连:    ${TAILSCALE_URL}`);
console.log(`   降级:    ${CLOUDFLARE_URL}`);
if (SERVER_URL_OVERRIDE) {
  console.log(`   覆盖:    ${SERVER_URL_OVERRIDE}`);
}
console.log(`   设备名:  ${os.hostname()}`);
console.log(`   心跳:    ${HEARTBEAT_INTERVAL / 1000}秒`);
console.log(`   CAM推送: ${CAM_STATE_INTERVAL / 1000}秒`);
console.log('   ─────────────────────────────────────────────');
console.log('   支持: exec / tmux / claude-code / cam-state');
console.log('   模式: Tailscale优先, Cloudflare降级');
console.log(`   本地面板: http://localhost:${LOCAL_PORT}`);
console.log('═══════════════════════════════════════════════════');
console.log('');

startLocalServer();
connect();
