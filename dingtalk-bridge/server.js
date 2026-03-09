const { DWClient, TOPIC_ROBOT } = require('dingtalk-stream');
const https = require('https');
const http = require('http');
const { WebSocketServer } = require('ws');
const crypto = require('crypto');

// ─── 配置 ───────────────────────────────────────────────────────────
const CLIENT_ID = process.env.DINGTALK_CLIENT_ID || '';
const CLIENT_SECRET = process.env.DINGTALK_CLIENT_SECRET || '';
const AI_API_KEY = process.env.AI_API_KEY || process.env.ANTHROPIC_AUTH_TOKEN || '';
const AI_BASE_URL = process.env.AI_BASE_URL || 'https://99code.jcylite.dpdns.org/v1';
const AI_MODEL = process.env.AI_MODEL || 'claude-sonnet-4-6';
const HEARTBEAT_PORT = parseInt(process.env.HEARTBEAT_PORT || '3211');
const DEVICE_TIMEOUT = parseInt(process.env.DEVICE_TIMEOUT || '90');
const WS_TOKEN = process.env.WS_TOKEN || 'dingtalk-bridge-2026';
const DINGTALK_USER_IDS = (process.env.DINGTALK_USER_IDS || '1024524').split(',').filter(Boolean);
const CAM_NOTIFY_TOKEN = process.env.CAM_NOTIFY_TOKEN || WS_TOKEN; // CAM 推送认证
const DINGTALK_AGENT_ID = process.env.DINGTALK_AGENT_ID || '4303703671'; // 工作通知 agentId

if (!CLIENT_ID || !CLIENT_SECRET || !AI_API_KEY) {
  console.error('❌ 请设置 DINGTALK_CLIENT_ID, DINGTALK_CLIENT_SECRET, AI_API_KEY');
  process.exit(1);
}

// ─── CAM Dashboard 状态 ──────────────────────────────────────────────
let camState = { agents: [], pending: { confirmations: [] }, timestamp: 0, macOnline: false, projectDirs: [] };
const sseClients = new Set();

// ─── 终端快照缓存（降级模式下由 Mac bridge 主动推送填充） ──────────────
const terminalSnapshotCache = new Map(); // session_name → { output, timestamp }
let deviceDegradedMode = false; // Mac bridge 是否处于降级模式

function broadcastSSE(data) {
  const msg = `data: ${JSON.stringify(data)}\n\n`;
  for (const res of sseClients) {
    try { res.write(msg); } catch { sseClients.delete(res); }
  }
}

// ─── 设备注册表 ─────────────────────────────────────────────────────
const devices = new Map();

function registerDevice(info, ws = null) {
  const key = info.id || info.name;
  const existing = devices.get(key);
  devices.set(key, {
    id: info.id || info.name,
    name: info.name || '未命名设备',
    type: info.type || 'unknown',
    ip: info.ip || '',
    hostname: info.hostname || '',
    os: info.os || '',
    cpu: info.cpu || '',
    memory: info.memory || '',
    disk: info.disk || '',
    uptime: info.uptime || '',
    extra: info.extra || {},
    capabilities: info.capabilities || [],
    ws: ws || (existing ? existing.ws : null),
    lastSeen: Date.now(),
    firstSeen: existing ? existing.firstSeen : Date.now(),
  });
}

function getOnlineDevices() {
  const now = Date.now();
  const online = [];
  for (const [, dev] of devices) {
    const ageSec = (now - dev.lastSeen) / 1000;
    if (ageSec <= DEVICE_TIMEOUT) {
      online.push({ ...dev, ws: undefined, ageSec });
    }
  }
  return online;
}

function getAllDevices() {
  const now = Date.now();
  const result = [];
  for (const [, dev] of devices) {
    const ageSec = (now - dev.lastSeen) / 1000;
    result.push({ ...dev, ws: undefined, ageSec, online: ageSec <= DEVICE_TIMEOUT, hasWs: !!dev.ws });
  }
  return result;
}

function findExecutableDevice(deviceHint) {
  const now = Date.now();
  if (deviceHint) {
    const hint = deviceHint.toLowerCase();
    for (const [key, dev] of devices) {
      if ((now - dev.lastSeen) / 1000 > DEVICE_TIMEOUT) continue;
      if (!dev.ws || dev.ws.readyState !== 1) continue;
      if (key.toLowerCase().includes(hint) || dev.name.toLowerCase().includes(hint) || dev.type.toLowerCase().includes(hint)) {
        return dev;
      }
    }
    return null;
  }
  for (const [, dev] of devices) {
    if ((now - dev.lastSeen) / 1000 > DEVICE_TIMEOUT) continue;
    if (dev.ws && dev.ws.readyState === 1) return dev;
  }
  return null;
}

function formatDuration(seconds) {
  if (seconds < 60) return `${Math.floor(seconds)}秒`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}分钟`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}小时${Math.floor((seconds % 3600) / 60)}分`;
  return `${Math.floor(seconds / 86400)}天${Math.floor((seconds % 86400) / 3600)}小时`;
}

function formatDeviceList() {
  const all = getAllDevices();
  if (all.length === 0) {
    return '📡 当前没有注册的设备。\n\n请在设备上运行桥接客户端来注册。';
  }

  const online = all.filter(d => d.online);
  const offline = all.filter(d => !d.online);

  let msg = `📡 设备状态报告\n\n`;
  msg += `在线: ${online.length} 台 | 离线: ${offline.length} 台\n`;
  msg += `${'─'.repeat(30)}\n`;

  if (online.length > 0) {
    msg += '\n🟢 在线设备:\n';
    for (const dev of online) {
      const onlineDur = formatDuration((Date.now() - dev.firstSeen) / 1000);
      msg += `\n  ✅ ${dev.name}`;
      if (dev.type) msg += ` (${dev.type})`;
      msg += ` ${dev.hasWs ? '🔗' : '📡'}`;
      msg += `\n     IP: ${dev.ip || '未知'}`;
      if (dev.os) msg += `\n     系统: ${dev.os}`;
      if (dev.memory) msg += `\n     内存: ${dev.memory}`;
      if (dev.disk) msg += `\n     磁盘: ${dev.disk}`;
      msg += `\n     在线: ${onlineDur}`;
      msg += `\n     最后心跳: ${Math.floor(dev.ageSec)}秒前`;
      if (dev.hasWs) msg += `\n     可执行命令: ✅`;
    }
  }

  if (offline.length > 0) {
    msg += '\n\n🔴 离线设备:\n';
    for (const dev of offline) {
      msg += `\n  ❌ ${dev.name}`;
      msg += ` — 离线 ${formatDuration(dev.ageSec)}`;
    }
  }

  return msg;
}

// ─── 钉钉 Access Token + 单聊推送 ──────────────────────────────────
let cachedToken = null;
let tokenExpiresAt = 0;

function getDingtalkAccessToken() {
  return new Promise((resolve, reject) => {
    if (cachedToken && Date.now() < tokenExpiresAt) {
      return resolve(cachedToken);
    }
    const body = JSON.stringify({ appKey: CLIENT_ID, appSecret: CLIENT_SECRET });
    const req = https.request('https://api.dingtalk.com/v1.0/oauth2/accessToken', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) },
    }, (res) => {
      let data = '';
      res.on('data', c => (data += c));
      res.on('end', () => {
        try {
          const r = JSON.parse(data);
          if (r.accessToken) {
            cachedToken = r.accessToken;
            tokenExpiresAt = Date.now() + (r.expireIn || 7200) * 1000 - 60000;
            resolve(cachedToken);
          } else {
            reject(new Error(`获取 token 失败: ${r.code} ${r.message}`));
          }
        } catch (e) { reject(e); }
      });
    });
    req.on('error', reject);
    req.write(body); req.end();
  });
}

function sendDingtalkOtoMessage(message, userIds = DINGTALK_USER_IDS) {
  return new Promise(async (resolve, reject) => {
    try {
      const token = await getDingtalkAccessToken();
      const body = JSON.stringify({
        robotCode: CLIENT_ID,
        userIds,
        msgKey: 'sampleText',
        msgParam: JSON.stringify({ content: message }),
      });
      const req = https.request('https://api.dingtalk.com/v1.0/robot/oToMessages/batchSend', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-acs-dingtalk-access-token': token,
          'Content-Length': Buffer.byteLength(body),
        },
      }, (res) => {
        let data = '';
        res.on('data', c => (data += c));
        res.on('end', () => {
          try {
            const r = JSON.parse(data);
            if (r.processQueryKey) {
              console.log(`📨 钉钉单聊发送成功: ${userIds.join(',')}`);
              resolve(r);
            } else {
              console.error(`❌ 钉钉单聊发送失败: ${r.code} ${r.message}`);
              reject(new Error(`${r.code}: ${r.message}`));
            }
          } catch (e) { reject(e); }
        });
      });
      req.on('error', reject);
      req.write(body); req.end();
    } catch (e) { reject(e); }
  });
}

// ─── 工作通知（有声音提醒） ──────────────────────────────────────────
function sendWorkNotification(message, userIds = DINGTALK_USER_IDS) {
  return new Promise(async (resolve, reject) => {
    try {
      const token = await getDingtalkAccessToken();
      const body = JSON.stringify({
        agent_id: parseInt(DINGTALK_AGENT_ID),
        userid_list: userIds.join(','),
        msg: { msgtype: 'text', text: { content: message } },
      });
      const url = `https://oapi.dingtalk.com/topapi/message/corpconversation/asyncsend_v2?access_token=${token}`;
      const req = https.request(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) },
      }, (res) => {
        let data = '';
        res.on('data', c => (data += c));
        res.on('end', () => {
          try {
            const r = JSON.parse(data);
            if (r.errcode === 0) {
              console.log(`🔔 工作通知发送成功: ${userIds.join(',')}`);
              resolve(r);
            } else {
              console.error(`❌ 工作通知发送失败: ${r.errcode} ${r.errmsg}`);
              reject(new Error(`${r.errcode}: ${r.errmsg}`));
            }
          } catch (e) { reject(e); }
        });
      });
      req.on('error', reject);
      req.write(body); req.end();
    } catch (e) { reject(e); }
  });
}

// 发送带声音提醒的通知：OTO（对话上下文）+ 工作通知（声音）
async function sendDingtalkWithSound(message, userIds = DINGTALK_USER_IDS) {
  const firstLine = message.split('\n')[0] || 'CAM 通知';
  await Promise.all([
    sendDingtalkOtoMessage(message, userIds).catch(err => {
      console.error(`⚠️ OTO 发送失败: ${err.message}`);
    }),
    sendWorkNotification(`🔔 ${firstLine}`, userIds).catch(err => {
      console.error(`⚠️ 工作通知发送失败: ${err.message}`);
    }),
  ]);
}

// ─── CAM 通知处理 + 自动审批 ────────────────────────────────────────
const pendingCamNotifications = new Map();
const recentCamNotifications = new Map(); // 去重: 同一事件指纹在极短时间内只发一次
const PENDING_EXPIRE_MS = 30 * 60 * 1000; // 30 分钟过期
const CAM_NOTIFY_DEDUP_MS = 5 * 1000; // 仅压制极短时间内的重复上报

function getCamNotificationFingerprint(event) {
  const eventType = event.eventType || 'unknown';

  if (event.context?.extractedMessage) {
    return `${eventType}::extracted::${event.context.extractedMessage}`;
  }

  if (eventType === 'permission_request') {
    const tool = event.eventData?.toolName || '';
    const cmd = event.eventData?.toolInput?.command || event.eventData?.toolInput?.file_path || '';
    const snapshot = event.context?.terminalSnapshot || '';
    return `${eventType}::${tool}::${cmd}::${snapshot}`;
  }

  if (eventType === 'waiting_for_input') {
    const patternType = event.eventData?.patternType || '';
    const snapshot = event.context?.terminalSnapshot || '';
    return `${eventType}::${patternType}::${snapshot}`;
  }

  if (eventType === 'error') {
    return `${eventType}::${event.eventData?.message || ''}`;
  }

  if (eventType === 'notification') {
    return `${eventType}::${event.eventData?.notificationType || ''}::${event.eventData?.message || ''}`;
  }

  return `${eventType}::${JSON.stringify(event.eventData || {})}`;
}

function buildCamNotificationDedupKey(event) {
  const agentId = event.agentId || 'unknown';
  return `${agentId}::${getCamNotificationFingerprint(event)}`;
}

function shouldDedupCamNotification(event) {
  const key = buildCamNotificationDedupKey(event);
  const now = Date.now();
  const lastSentAt = recentCamNotifications.get(key);
  if (lastSentAt && now - lastSentAt < CAM_NOTIFY_DEDUP_MS) {
    return true;
  }
  recentCamNotifications.set(key, now);

  for (const [existingKey, ts] of recentCamNotifications) {
    if (now - ts > CAM_NOTIFY_DEDUP_MS) recentCamNotifications.delete(existingKey);
  }
  return false;
}

// 自动清理过期 pending
setInterval(() => {
  const now = Date.now();
  for (const [key, entry] of pendingCamNotifications) {
    if (now - entry.timestamp > PENDING_EXPIRE_MS) {
      pendingCamNotifications.delete(key);
      console.log(`🗑️ CAM pending 过期: ${key}`);
    }
  }
  for (const [key, ts] of recentCamNotifications) {
    if (now - ts > CAM_NOTIFY_DEDUP_MS) {
      recentCamNotifications.delete(key);
    }
  }
}, 60_000);

const AUTO_APPROVE_WHITELIST = [
  /^ls\b/, /^cat\b/, /^head\b/, /^tail\b/, /^wc\b/, /^grep\b/, /^rg\b/,
  /^pwd$/, /^echo\b/, /^which\b/, /^whoami$/, /^date$/, /^uname\b/,
  /^git\s+(status|log|diff|branch|show|stash\s+list)\b/,
  /^cargo\s+(test|check|build|clippy|fmt|doc)\b/,
  /^npm\s+(test|run\s+(lint|check|build|dev|start))\b/,
  /^node\s+--version$/, /^npm\s+--version$/, /^rustc\s+--version$/,
  /^find\b/, /^tree\b/, /^du\b/, /^df\b/, /^top\b/, /^ps\b/,
  /^curl\s.*localhost/, /^open\b/,
];

const MUST_CONFIRM_BLACKLIST = [
  /\brm\s/, /\bsudo\b/, /\bmkfs\b/, /\bdd\b/, /\bchmod\s+777/,
  /&&/, /\|/, />>?/, /;/,
  /\bgit\s+(push|reset|rebase|force|checkout\s+\.)\b/,
  /\bnpm\s+(publish|unpublish)\b/,
  /\bcargo\s+publish\b/,
];

function shouldAutoApprove(event) {
  if (event.eventType !== 'permission_request') return false;
  const toolName = event.eventData?.toolName;
  if (toolName !== 'Bash') return false;

  const command = event.eventData?.toolInput?.command;
  if (!command) return false;

  // 黑名单优先
  if (MUST_CONFIRM_BLACKLIST.some(re => re.test(command))) return false;
  // 白名单
  if (AUTO_APPROVE_WHITELIST.some(re => re.test(command))) return true;
  // 默认需要人工
  return false;
}

function formatCamNotification(event) {
  const emoji = event.urgency === 'HIGH' ? '🔴' : event.urgency === 'MEDIUM' ? '🟡' : '🟢';
  const labels = {
    permission_request: '权限请求',
    waiting_for_input: '待确认',
    error: '错误',
    agent_exited: 'Agent 退出',
    notification: '通知',
  };
  const label = labels[event.eventType] || event.eventType;
  const agentId = event.agentId || 'unknown';
  const project = event.projectPath || 'unknown';
  const progress = event.progress;

  let detail = '';
  if (event.context?.extractedMessage) {
    detail = event.context.extractedMessage;
  } else if (event.eventType === 'permission_request') {
    const tool = event.eventData?.toolName || '';
    const cmd = event.eventData?.toolInput?.command || event.eventData?.toolInput?.file_path || '';
    const snapshot = event.context?.terminalSnapshot;
    if (snapshot) {
      const lines = snapshot.split('\n');
      detail = `${tool} ${cmd}\n\n${lines.slice(-30).join('\n')}`;
    } else {
      detail = `${tool} ${cmd}`;
    }
  } else if (event.eventType === 'error') {
    detail = event.eventData?.message || '';
  } else if (event.eventType === 'waiting_for_input') {
    if (event.context?.terminalSnapshot) {
      const lines = event.context.terminalSnapshot.split('\n');
      detail = lines.slice(-20).join('\n');
    }
  }

  let msg = `${emoji} [CAM] ${agentId}\n${label} | ${event.urgency}\n项目: ${project}`;
  if (detail) msg += `\n\n${detail}`;

  if (progress && typeof progress.completedTasks === 'number' && typeof progress.totalTasks === 'number') {
    const completionRate = typeof progress.completionRate === 'number'
      ? progress.completionRate
      : (progress.totalTasks > 0 ? Math.round((progress.completedTasks / progress.totalTasks) * 100) : 0);
    msg += `\n\n📊 进度: ${progress.completedTasks}/${progress.totalTasks} (${completionRate}%)`;

    if (Array.isArray(progress.remainingTop3) && progress.remainingTop3.length > 0) {
      msg += '\n剩余重点:';
      progress.remainingTop3.forEach((item, index) => {
        const subject = item?.subject || '';
        if (subject) msg += `\n  ${index + 1}. ${subject}`;
      });
    }

    if ((event.eventType === 'permission_request' || event.eventType === 'waiting_for_input') && progress.needsConfirmation) {
      msg += `\n⏳ 待确认项: ${progress.pendingConfirmationsCount || 0}`;
    }
  }

  if (event.eventType === 'permission_request') {
    msg += '\n\n回复 y 允许 / n 拒绝';
  } else if (event.eventType === 'waiting_for_input') {
    msg += '\n\n回复你的选择或输入';
  }

  return msg;
}

async function handleCamNotify(event) {
  console.log(`\n📥 CAM 通知: ${event.eventType} agent=${event.agentId} urgency=${event.urgency}`);

  // stop/session_end/agent_exited → 统一走 handleAgentExit（AI 摘要 + 去重）
  const evtNorm = (event.eventType || '').toLowerCase().replace(/_/g, '');
  if (evtNorm === 'stop' || evtNorm === 'sessionend' || evtNorm === 'agentexited') {
    const agent = {
      agent_id: event.agentId,
      tmux_session: event.agentId,
      project_path: event.projectPath,
      started_at: event.startedAt,
    };
    await handleAgentExit(agent, event.agentId);
    return { ok: true, action: 'exit_handled' };
  }

  // 自动审批检查
  if (shouldAutoApprove(event)) {
    const cmd = event.eventData?.toolInput?.command || '';
    console.log(`✅ 自动审批: ${cmd}`);

    // 通过 WebSocket 发送 cam reply 到 Mac
    try {
      await sendDeviceRequest(null, {
        type: 'cam_reply',
        agentId: event.agentId,
        input: 'y',
      }, 10000);
      return { ok: true, action: 'auto_approved', command: cmd };
    } catch (err) {
      console.error(`❌ 自动审批回复失败: ${err.message}, 降级为人工`);
      // 降级为人工确认
    }
  }

  // 钉钉通知由本地 CAM 直发，server 只负责 Dashboard 状态 + pending 管理
  const message = formatCamNotification(event);

  // 记录 pending（用于回复路由）
  if (event.eventType === 'permission_request' || event.eventType === 'waiting_for_input') {
    pendingCamNotifications.set(event.agentId, {
      event,
      timestamp: Date.now(),
      message,
    });
    console.log(`📋 Pending 记录: ${event.agentId} (共 ${pendingCamNotifications.size} 个)`);
  }

  // 广播到 Dashboard SSE
  broadcastSSE({ type: 'cam_notify', event });
  console.log(`📡 CAM 通知已广播到 Dashboard (钉钉由本地 CAM 直发)`);
  return { ok: true, action: 'dashboard_only', agentId: event.agentId };
}

// 查找匹配的 pending CAM 回复
function findPendingCamReply(userInput) {
  if (pendingCamNotifications.size === 0) return null;
  const input = userInput.trim();

  // 格式: "cam-xxx: y" 或 "cam-xxx y"
  const match = input.match(/^(cam-\S+)[:\s]+(.+)$/i);
  if (match) {
    const agentId = match[1];
    const reply = match[2].trim();
    const pending = pendingCamNotifications.get(agentId);
    if (pending) {
      pendingCamNotifications.delete(agentId);
      return { agentId, reply };
    }
  }

  // 只有一个 pending 时，短回复直接路由
  if (pendingCamNotifications.size === 1 && input.length <= 20) {
    const [agentId, pending] = [...pendingCamNotifications.entries()][0];
    pendingCamNotifications.delete(agentId);
    return { agentId, reply: input };
  }

  return null;
}

// 通过 WebSocket 发送 CAM 回复到 Mac Bridge
async function sendCamReplyToDevice(agentId, input) {
  console.log(`📤 转发 CAM 回复: agent=${agentId}, input=${input}`);
  pendingCamNotifications.delete(agentId);
  try {
    const result = await sendDeviceRequest(null, {
      type: 'cam_reply',
      agentId,
      input,
    }, 10000);
    return result;
  } catch (err) {
    console.error(`❌ CAM 回复转发失败: ${err.message}`);
    throw err;
  }
}

// ─── 远程命令执行（通用） ─────────────────────────────────────────────
const pendingCommands = new Map();

const COMMAND_BLACKLIST = [
  /\brm\s+(-\w*\s+)*-\w*[rR]\w*\s+\//,
  /\bsudo\b/,
  /\bmkfs\b/,
  /\bdd\s+.*of=\/dev/,
  /\b(shutdown|reboot|halt|poweroff)\b/,
  />\s*\/dev\/sd/,
  /\bchmod\s+777\s+\//,
  /\bformat\b.*[cC]:/,
];

function isCommandSafe(command) {
  for (const re of COMMAND_BLACKLIST) {
    if (re.test(command)) return false;
  }
  return true;
}

// 通用的设备请求：发送消息并等待响应
function sendDeviceRequest(deviceHint, message, timeout = 30000) {
  return new Promise((resolve, reject) => {
    const device = findExecutableDevice(deviceHint || null);
    if (!device) {
      return reject(new Error(deviceHint ? `设备 "${deviceHint}" 不在线` : '没有可用的远程设备'));
    }

    const reqId = crypto.randomUUID();
    const timer = setTimeout(() => {
      pendingCommands.delete(reqId);
      reject(new Error(`请求超时 (${timeout / 1000}秒)`));
    }, timeout);

    pendingCommands.set(reqId, { resolve, reject, timer });
    device.ws.send(JSON.stringify({ ...message, id: reqId }));
  });
}

function executeOnDevice(deviceId, command, timeout = 30000) {
  if (!isCommandSafe(command)) {
    return Promise.reject(new Error(`命令被安全策略阻止: ${command}`));
  }
  return sendDeviceRequest(deviceId, { type: 'exec', command, timeout }, timeout);
}

// ─── 内置命令检测 ───────────────────────────────────────────────────
const DEVICE_COMMANDS = [
  /设备.*(在线|状态|列表|信息)/,
  /在线.*设备/,
  /(哪些|什么).*(设备|机器|电脑|服务器)/,
  /^设备$/,
  /device.*(status|list|online)/i,
];

function isDeviceCommand(text) {
  return DEVICE_COMMANDS.some(re => re.test(text));
}

// ─── AI Tool Use (Anthropic format) ─────────────────────────────────
const TOOLS = [
  {
    name: 'execute_command',
    description: '在用户的远程设备上执行 shell 命令。用于查看文件、查询系统信息等简单操作。',
    input_schema: {
      type: 'object',
      properties: {
        command: { type: 'string', description: '要执行的 shell 命令' },
        device: { type: 'string', description: '目标设备名称（可选）' },
      },
      required: ['command'],
    },
  },
  {
    name: 'list_devices',
    description: '查看当前所有在线设备的详细信息。',
    input_schema: {
      type: 'object',
      properties: {},
    },
  },
  {
    name: 'start_claude_code',
    description: '在远程设备上启动一个 Claude Code 会话来执行复杂的编码任务。会在指定目录下创建 tmux 会话，启动 Claude Code，并发送任务 prompt。适合需要多步骤编码、调试、重构等复杂开发任务。',
    input_schema: {
      type: 'object',
      properties: {
        working_directory: { type: 'string', description: '工作目录，Claude Code 将在此目录启动，如 ~/project/my-app' },
        prompt: { type: 'string', description: '发送给 Claude Code 的任务描述，如 "完成用户登录功能的开发"' },
        session_name: { type: 'string', description: 'tmux 会话名称（可选，默认自动生成）' },
        device: { type: 'string', description: '目标设备（可选）' },
      },
      required: ['working_directory', 'prompt'],
    },
  },
  {
    name: 'send_to_tmux',
    description: '向已有的 tmux 会话发送文本输入。可以用来给运行中的 Claude Code 发送追加指令、回复确认（如 y/n）等。',
    input_schema: {
      type: 'object',
      properties: {
        session: { type: 'string', description: 'tmux 会话名称' },
        text: { type: 'string', description: '要发送的文本' },
        press_enter: { type: 'boolean', description: '发送后是否按回车（默认 true）' },
        device: { type: 'string', description: '目标设备（可选）' },
      },
      required: ['session', 'text'],
    },
  },
  {
    name: 'check_tmux_output',
    description: '查看 tmux 会话的最近输出，用来检查 Claude Code 的执行进度和结果。',
    input_schema: {
      type: 'object',
      properties: {
        session: { type: 'string', description: 'tmux 会话名称' },
        lines: { type: 'number', description: '获取最近多少行输出（默认 50）' },
        device: { type: 'string', description: '目标设备（可选）' },
      },
      required: ['session'],
    },
  },
  {
    name: 'list_tmux_sessions',
    description: '列出远程设备上所有运行中的 tmux 会话，查看哪些编码任务正在进行。',
    input_schema: {
      type: 'object',
      properties: {
        device: { type: 'string', description: '目标设备（可选）' },
      },
    },
  },
];

function buildSystemPrompt() {
  const projectDirs = camState.projectDirs || [];
  const projectList = projectDirs.length > 0
    ? projectDirs.map(d => `  ~/project/${d}`).join('\n')
    : '  (暂无项目列表，用 execute_command 执行 ls ~/project/ 查询)';

  const runningAgents = (camState.agents || [])
    .map(a => `  ${a.agent_id} → ${a.project_path || '?'} [${a.status || '?'}]`)
    .join('\n') || '  (无)';

  return `你是钉钉群里的远程助手，能在用户的 Mac 上执行命令和管理 Claude Code 编码任务。

绝对禁止（违反任何一条视为严重错误）：
- 禁止编造任何数据。你不知道任务状态、文件内容、代码进度。只有工具返回的数据是真实的。
- 禁止说"已启动"、"已完成"等结果描述，除非你刚刚调用了工具并拿到了成功返回。
- 禁止 Markdown 格式（**粗体**、\`代码\`、---）。钉钉只显示纯文本。
- 禁止写模板或占位符。

你必须这样工作：
1. 用户问任务/进度/状态 → 必须先调工具，拿到真实数据后再回答
2. 工具调用失败 → 如实说"失败"，不要猜测
3. 回复简短直接

═══ 启动会话规则（最重要！）═══

当用户要求启动会话/研究问题/修bug/做功能时，你必须：
1. 直接调用 start_claude_code，不要问用户任何问题
2. working_directory：从下面的项目列表模糊匹配。用户说"openclaw"就匹配含 openclaw 的目录。无法匹配时用 execute_command 执行 ls ~/project/ 查找，仍找不到才用 ~/project
3. session_name：从用户描述提取2-4个关键词，用连字符拼接，如 fix-login-bug、openclaw-origin-fix
4. prompt：直接使用用户的原始描述

绝对不要问"请告诉我项目路径"或"你的代码在哪个目录"！自己匹配！

Mac 上的项目目录：
${projectList}

当前运行中的 Agent：
${runningAgents}

═══ 工具使用规则 ═══
- 任务列表 → list_tmux_sessions
- 任务进度 → check_tmux_output（传 session 名称）
- 启动编码任务 → start_claude_code（不要传 device 参数）
- 执行命令 → execute_command
- 追加指令/确认 → send_to_tmux

设备说明：
- Mac：支持所有工具（exec/tmux/claude-code）
- NAS 等设备：仅支持 execute_command
- 不要把 start_claude_code 发给非 Mac 设备`;
}

// ─── 对话历史（持久化） ───────────────────────────────────────────────────────
const chatHistory = new Map();
const MAX_HISTORY = 20;
const CHAT_HISTORY_FILE = require('path').join(__dirname, 'chat-history.json');

// 启动时加载历史
try {
  const saved = JSON.parse(require('fs').readFileSync(CHAT_HISTORY_FILE, 'utf-8'));
  for (const [k, v] of Object.entries(saved)) {
    chatHistory.set(k, v);
  }
  console.log(`📂 加载聊天历史: ${chatHistory.size} 个会话`);
} catch { /* 首次运行无文件 */ }

function saveChatHistory() {
  try {
    const obj = {};
    for (const [k, v] of chatHistory) obj[k] = v;
    require('fs').writeFileSync(CHAT_HISTORY_FILE, JSON.stringify(obj, null, 2));
  } catch (e) { console.error('保存聊天历史失败:', e.message); }
}

async function handleToolCall(toolBlock) {
  const name = toolBlock.name;
  const args = toolBlock.input || {};

  if (name === 'execute_command') {
    const { command, device } = args;
    if (!command) return JSON.stringify({ error: '缺少 command 参数' });
    try {
      const result = await executeOnDevice(device || null, command);
      let output = result.stdout || '';
      if (output.length > 3000) {
        output = output.substring(0, 3000) + `\n... (输出已截断，共 ${result.stdout.length} 字符)`;
      }
      return JSON.stringify({
        stdout: output,
        stderr: result.stderr || '',
        exitCode: result.exitCode,
        device: result.device,
      });
    } catch (err) {
      return JSON.stringify({ error: err.message });
    }
  }

  if (name === 'list_devices') {
    return JSON.stringify({ devices: getAllDevices() });
  }

  if (name === 'start_claude_code') {
    const { working_directory, prompt, session_name, device } = args;
    if (!working_directory || !prompt) return JSON.stringify({ error: '缺少 working_directory 或 prompt' });
    // 只允许有 claude-code 能力的设备（Mac）执行
    const targetDevice = findExecutableDevice(device || null);
    if (!targetDevice) {
      return JSON.stringify({ error: device ? `设备 "${device}" 不在线` : '没有可用的远程设备' });
    }
    const caps = targetDevice.capabilities || [];
    if (!caps.includes('claude-code') && !caps.includes('tmux')) {
      const macDevices = getOnlineDevices().filter(d => (d.capabilities || []).includes('claude-code'));
      if (macDevices.length > 0) {
        return JSON.stringify({ error: `设备 "${targetDevice.name}" 不支持 Claude Code，请使用 Mac 设备（可用: ${macDevices.map(d => d.id).join(', ')}）` });
      }
      return JSON.stringify({ error: `设备 "${targetDevice.name}" 不支持 Claude Code。只有 Mac 设备才能运行 Claude Code，当前无 Mac 在线。` });
    }
    try {
      const result = await sendDeviceRequest(device || null, {
        type: 'tmux_start_claude',
        working_directory,
        prompt,
        session_name: session_name || '',
      }, 15000);
      return JSON.stringify(result);
    } catch (err) {
      return JSON.stringify({ error: err.message });
    }
  }

  if (name === 'send_to_tmux') {
    const { session, text, press_enter, device } = args;
    if (!session || text === undefined) return JSON.stringify({ error: '缺少 session 或 text' });
    try {
      const result = await sendDeviceRequest(device || null, {
        type: 'tmux_send',
        session,
        text,
        press_enter: press_enter !== false,
      }, 10000);
      return JSON.stringify(result);
    } catch (err) {
      return JSON.stringify({ error: err.message });
    }
  }

  if (name === 'check_tmux_output') {
    const { session, lines, device } = args;
    if (!session) return JSON.stringify({ error: '缺少 session' });
    try {
      const result = await sendDeviceRequest(device || null, {
        type: 'tmux_capture',
        session,
        lines: lines || 50,
      }, 10000);
      return JSON.stringify(result);
    } catch (err) {
      return JSON.stringify({ error: err.message });
    }
  }

  if (name === 'list_tmux_sessions') {
    const { device } = args;
    try {
      const result = await sendDeviceRequest(device || null, {
        type: 'tmux_list',
      }, 10000);
      return JSON.stringify(result);
    } catch (err) {
      return JSON.stringify({ error: err.message });
    }
  }

  return JSON.stringify({ error: `未知工具: ${name}` });
}

function aiRequest(body) {
  return new Promise((resolve, reject) => {
    const payload = JSON.stringify(body);
    const url = new URL(`${AI_BASE_URL}/messages`);
    const transport = url.protocol === 'https:' ? https : http;

    console.log(`📡 AI 请求: ${url.href}, model=${body.model}, messages=${body.messages?.length}`);

    const req = transport.request(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'x-api-key': AI_API_KEY,
        'anthropic-version': '2023-06-01',
        'Content-Length': Buffer.byteLength(payload),
      },
    }, (res) => {
      let data = '';
      res.on('data', c => (data += c));
      res.on('end', () => {
        console.log(`📡 AI API 响应: HTTP ${res.statusCode}, ${data.length} 字符`);
        if (data.length < 500) console.log(`📡 原始响应: ${data}`);
        else console.log(`📡 响应前500字符: ${data.substring(0, 500)}`);
        try {
          const result = JSON.parse(data);
          if (result.error) return reject(new Error(result.error.message || JSON.stringify(result.error)));
          resolve(result);
        } catch (e) {
          console.error(`❌ JSON 解析失败: ${e.message}`);
          reject(new Error(`API 响应解析失败 (HTTP ${res.statusCode}): ${data.substring(0, 100)}`));
        }
      });
    });
    req.on('error', (e) => {
      console.error(`❌ AI 请求网络错误: ${e.message}`);
      reject(e);
    });
    req.setTimeout(120_000, () => { req.destroy(); reject(new Error('AI 请求超时')); });
    req.write(payload);
    req.end();
  });
}

// ─── @提及消息分析 ────────────────────────────────────────────────
async function analyzeAtMention({ content, senderNick, conversationTitle, atUsers }) {
  const atNames = (atUsers || []).map(u => u.senderNick || u.staffId || u.dingtalkId).filter(Boolean);
  const prompt = `有人在钉钉群里@了我，请帮我快速分析这条消息。

群名: ${conversationTitle || '未知'}
发送者: ${senderNick}
被@的人: ${atNames.join(', ') || '我'}
消息内容: ${content}

请用纯文本回复（禁止 Markdown），格式：
[群名] 发送者@了你
要点: 一句话总结消息核心内容
紧急度: 高/中/低
建议: 需要立即回复 / 稍后处理 / 仅通知无需回复
如需回复，建议回复内容: (简短建议)`;

  try {
    const result = await aiRequest({
      model: AI_MODEL,
      system: '你是一个消息分析助手。用户会收到群里@他的消息，你需要快速分析并给出简要报告。回复必须简短、直接、纯文本格式。',
      messages: [{ role: 'user', content: prompt }],
      max_tokens: 500,
    });
    const text = (result.content || []).filter(b => b.type === 'text').map(b => b.text).join('\n');
    return text || '(分析失败)';
  } catch (err) {
    console.error(`❌ @消息分析失败: ${err.message}`);
    return `[${conversationTitle || '群聊'}] ${senderNick} @了你: ${content.substring(0, 200)}`;
  }
}

// ─── Agent 完成摘要提取 ────────────────────────────────────────────
async function extractCompletionSummary(terminalOutput, agentId, project) {
  try {
    // 清理 ANSI 转义码
    const cleaned = terminalOutput
      .replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '')
      .replace(/\x1b\][^\x07]*\x07/g, '')
      .trim();

    // 只取最后 4000 字符，避免 token 过多
    const tail = cleaned.length > 4000 ? cleaned.substring(cleaned.length - 4000) : cleaned;

    const result = await aiRequest({
      model: 'claude-haiku-4-5-20251001',
      max_tokens: 500,
      messages: [{
        role: 'user',
        content: `你是终端输出分析器。以下是 AI 编码 Agent "${agentId}" 在项目 "${project}" 中完成工作后的终端输出。

请提取工作摘要，格式要求：
1. 用纯文本，不要 Markdown 格式
2. 第一行用一句话概括完成了什么
3. 如果有具体改动，列出关键文件或功能（最多 5 项，每项一行，用 - 开头）
4. 如果有 git commit，提取 commit message
5. 如果有测试结果，简述通过/失败情况
6. 总共不超过 8 行

重要：如果以下任何情况为真，你必须只回复 NONE（不要解释原因）：
- 终端只有空提示符（❯ 或 $ 或 %）、启动画面、loading 动画（Slithering/Hatching/Brewing/Thinking 等）
- 只看到 Agent 启动/连接信息但没有实质工作产出
- 无法判断 Agent 完成了什么有价值的事
- Agent 正在处理中、尚未完成

回复格式：有摘要就直接输出摘要文本，无摘要就只回复 NONE

终端输出：
\`\`\`
${tail}
\`\`\``
      }],
    });

    const text = result.content?.[0]?.text?.trim() || '';
    // AI 返回 NONE 或空字符串表示没有有意义的内容
    if (!text || text === 'NONE' || text === '""' || text === "''" || text.startsWith('空字符串')) return '';
    return text;
  } catch (err) {
    console.error(`⚠️ AI 摘要提取失败: ${err.message}`);
    return '';
  }
}

// ─── Agent 退出/完成统一处理 ─────────────────────────────────────────
const recentExitNotifications = new Map(); // agentId → timestamp (5 分钟去重)

function getAgentKeys(agent, agentId) {
  const keys = new Set();
  for (const value of [
    agentId,
    agent?.agent_id,
    agent?.tmux_session,
    agent?.session_id,
    agent?.id,
  ]) {
    if (typeof value === 'string' && value.trim()) keys.add(value.trim());
  }
  return [...keys];
}

function hasPendingForAgent(agent, agentId) {
  const keys = getAgentKeys(agent, agentId);
  if (keys.length === 0) return false;

  for (const key of keys) {
    if (pendingCamNotifications.has(key)) return true;
  }

  const confirmations = camState?.pending?.confirmations || [];
  return confirmations.some(item => {
    const values = [
      item?.agent_id,
      item?.agentId,
      item?.tmux_session,
      item?.session,
      item?.session_id,
      item?.id,
    ]
      .filter(v => typeof v === 'string' && v.trim())
      .map(v => v.trim());
    return values.some(v => keys.includes(v));
  });
}

async function handleAgentExit(agent, agentId) {
  console.log(`\n📋 handleAgentExit 开始: ${agentId}`);
  console.log(`   agent 数据: started_at=${agent.started_at}, project=${agent.project_path}, tmux=${agent.tmux_session}`);

  if (hasPendingForAgent(agent, agentId)) {
    console.log(`⏸️ Agent ${agentId} 仍有待确认/待输入事项，跳过完成通知`);
    return;
  }

  // 去重：同一 agent 5 分钟内不重复通知
  const lastNotified = recentExitNotifications.get(agentId);
  if (lastNotified && Date.now() - lastNotified < 5 * 60 * 1000) {
    console.log(`🔇 Agent ${agentId} 已在 5 分钟内通知过，跳过 (距上次 ${Math.round((Date.now() - lastNotified) / 1000)}秒)`);
    return;
  }

  const startedAt = agent.started_at ? new Date(agent.started_at) : null;
  const durationSec = startedAt ? (Date.now() - startedAt.getTime()) / 1000 : 0;
  const duration = startedAt ? formatDuration(durationSec) : '未知';
  const project = agent.project_path || '未知项目';
  const shortProject = project.replace(/^\/Users\/\w+\//, '~/');
  const session = agent.tmux_session || agentId;
  const link = `https://openclaw.jcylite.dpdns.org/dashboard#terminal/${session}`;

  console.log(`   运行时长: ${duration}, session: ${session}`);

  // 尝试抓终端摘要（AI 判断是否有有价值的工作内容）
  let summary = '';
  try {
    console.log(`   正在抓取终端 ${session} 最后 150 行...`);
    const captureResult = await sendDeviceRequest(null, {
      type: 'tmux_capture', session, lines: 150,
    }, 10000);
    const termOutput = (captureResult?.data?.output || captureResult?.output || '').trim();
    console.log(`   终端输出长度: ${termOutput.length} 字符`);
    if (termOutput) {
      console.log(`   调用 AI 提取摘要...`);
      summary = await extractCompletionSummary(termOutput, agentId, shortProject);
      console.log(`   AI 摘要结果: ${summary ? `"${summary.substring(0, 80)}..."` : '(空)'}`);
    } else {
      console.log(`   终端输出为空`);
    }
  } catch (err) {
    console.error(`⚠️ 退出摘要提取失败: ${err.message}`);
  }

  // 无摘要 = 不发通知（AI 判定无有价值内容）
  if (!summary) {
    console.log(`🔇 Agent ${agentId} 无有效工作摘要，跳过通知`);
    return;
  }

  // 有摘要，发送有价值的通知
  const lines = [`✅ [Agent 完成]`, agentId, `项目: ${shortProject}`, `时长: ${duration}`, '', summary, '', `终端: ${link}`];
  const message = lines.join('\n');
  recentExitNotifications.set(agentId, Date.now());
  // 清理过期条目
  for (const [id, ts] of recentExitNotifications) {
    if (Date.now() - ts > 10 * 60 * 1000) recentExitNotifications.delete(id);
  }

  // 钉钉通知由本地 CAM 直发，server 只广播到 Dashboard
  broadcastSSE({ type: 'agent_exit', agentId, message });
  console.log(`📡 Agent 退出已广播到 Dashboard (钉钉由本地 CAM 直发): ${agentId}`);
}

// 检测用户消息是否在询问任务状态（需要强制工具调用）
function isStatusQuery(text) {
  const patterns = [
    /完成|进度|进展|状态|怎么样|做得|做的|搞得|搞的/,
    /多少.*任务|任务.*多少|几个.*任务|任务.*几个/,
    /在.*做什么|做.*什么|干什么|在干/,
    /哪个.*分支|分支.*哪个|git.*变更|变更/,
    /有没有.*写|写了.*没|开始.*没|启动.*没/,
  ];
  return patterns.some(p => p.test(text));
}

async function askAI(message, senderId) {
  if (!chatHistory.has(senderId)) chatHistory.set(senderId, []);
  const history = chatHistory.get(senderId);
  history.push({ role: 'user', content: message });
  while (history.length > MAX_HISTORY) history.shift();

  const hasExecutableDevice = !!findExecutableDevice(null);

  // 根因 C 修复：设备离线时直接告知，不让 AI 编造
  if (!hasExecutableDevice) {
    const onlineDevs = getOnlineDevices();
    console.log(`⚠️ 无可执行设备! 在线设备数=${onlineDevs.length}, 详情=${JSON.stringify(onlineDevs.map(d => ({ id: d.id, hasWs: !!d.ws, caps: d.capabilities })))}`);
    const reply = '当前没有在线设备，无法查询任务状态或执行命令。请确认设备上的桥接客户端正在运行。';
    history.push({ role: 'assistant', content: reply });
    return reply;
  }

  console.log(`✅ 找到可执行设备, 在线设备数=${getOnlineDevices().length}`);

  // 根因 B 修复：清除历史中的"设备离线"污染消息，避免 AI 照搬
  for (let i = history.length - 1; i >= 0; i--) {
    if (history[i].role === 'assistant' && typeof history[i].content === 'string' &&
        history[i].content.includes('没有在线设备')) {
      history.splice(i, 1);
    }
  }

  // 根因 B 修复：状态查询时，在消息中注入强制工具调用指令
  let messages = [...history];
  if (isStatusQuery(message)) {
    const lastMsg = messages[messages.length - 1];
    if (typeof lastMsg.content === 'string') {
      messages[messages.length - 1] = {
        ...lastMsg,
        content: lastMsg.content + '\n\n[系统指令] 用户在询问任务状态。你必须先调用 list_tmux_sessions 或 check_tmux_output 获取真实数据，严禁凭记忆回答。',
      };
    }
  }

  // Tool use 循环：最多 10 轮
  // 收集所有中间文本，最终一起返回
  const intermediateTexts = [];

  for (let i = 0; i < 10; i++) {
    const body = {
      model: AI_MODEL,
      system: buildSystemPrompt(),
      messages,
      max_tokens: 2000,
      tools: TOOLS,
    };

    const result = await aiRequest(body);

    const stopReason = result.stop_reason;
    const content = result.content || [];

    console.log(`📡 stop_reason: ${stopReason}, content blocks: ${content.length}`);

    const textBlocks = content.filter(b => b.type === 'text');
    const toolBlocks = content.filter(b => b.type === 'tool_use');

    // 收集中间思考文本（不丢弃）
    if (textBlocks.length > 0) {
      const text = textBlocks.map(b => b.text).join('\n').trim();
      if (text) {
        console.log(`💬 中间文本: ${text.substring(0, 100)}...`);
        intermediateTexts.push(text);
      }
    }

    if (toolBlocks.length === 0) {
      // 最后一轮，合并所有中间文本 + 最终回复
      const finalText = textBlocks.map(b => b.text).join('\n') || '';
      // 去掉 intermediateTexts 最后一条（因为已经在 finalText 里了）
      if (intermediateTexts.length > 0 && finalText) {
        intermediateTexts.pop();
      }
      const allParts = [...intermediateTexts, finalText].filter(t => t.trim());
      const reply = allParts.join('\n\n') || '(无回复)';
      // 根因 A 修复：只保存简短回复到历史，避免长文本污染
      const historyReply = reply.length > 500 ? reply.substring(0, 500) + '...' : reply;
      history.push({ role: 'assistant', content: historyReply });
      saveChatHistory();
      return reply;
    }

    messages.push({ role: 'assistant', content });
    console.log(`🔧 AI 调用了 ${toolBlocks.length} 个工具`);

    const toolResults = [];
    for (const tb of toolBlocks) {
      console.log(`   → ${tb.name}(${JSON.stringify(tb.input)})`);
      const toolResult = await handleToolCall(tb);
      toolResults.push({
        type: 'tool_result',
        tool_use_id: tb.id,
        content: toolResult,
      });
    }

    messages.push({ role: 'user', content: toolResults });
  }

  return '工具调用次数过多，请简化你的请求。';
}

// ─── 知识挖掘 AI ─────────────────────────────────────────────────────
const kbChatHistory = new Map(); // sessionId -> messages[]

const KB_SYSTEM_PROMPT = `你是一个知识分析师。用户会基于知识库中的文件与你对话，你可以：
1. 读取知识库文件内容进行分析
2. 列出可用的知识文件
3. 将分析结果保存为新的知识文件

你的回答应该准确、有深度，基于文件的实际内容。如果用户没有指定文件，先列出可用文件让用户选择。
当需要保存分析结果时，使用 save_kb_file 工具，文件名应有意义且以 .md 结尾。`;

const KB_TOOLS = [
  {
    name: 'read_kb_file',
    description: '读取知识库中的文件内容',
    input_schema: {
      type: 'object',
      properties: { filename: { type: 'string', description: '文件名' } },
      required: ['filename'],
    },
  },
  {
    name: 'list_kb_files',
    description: '列出知识库中所有可用文件',
    input_schema: { type: 'object', properties: {} },
  },
  {
    name: 'save_kb_file',
    description: '将内容保存为新的知识文件',
    input_schema: {
      type: 'object',
      properties: {
        filename: { type: 'string', description: '文件名（建议 .md 结尾）' },
        content: { type: 'string', description: '文件内容' },
      },
      required: ['filename', 'content'],
    },
  },
];

function handleKbToolCall(tb) {
  const fs = require('fs');
  const kbDir = '/kb';
  const { name, input } = tb;

  if (name === 'list_kb_files') {
    try {
      const entries = fs.readdirSync(kbDir);
      const files = entries.map(n => {
        try {
          const stat = fs.statSync(`${kbDir}/${n}`);
          if (!stat.isFile()) return null;
          const size = stat.size < 1024 ? `${stat.size}B` : stat.size < 1048576 ? `${(stat.size/1024).toFixed(1)}K` : `${(stat.size/1048576).toFixed(1)}M`;
          return { name: n, size, date: stat.mtime.toISOString().substring(0, 10) };
        } catch { return null; }
      }).filter(Boolean);
      return JSON.stringify({ files });
    } catch (err) {
      return JSON.stringify({ error: err.message });
    }
  }

  if (name === 'read_kb_file') {
    const fn = input.filename;
    if (!fn || fn.includes('..') || fn.includes('/')) return JSON.stringify({ error: '非法文件名' });
    try {
      const content = fs.readFileSync(`${kbDir}/${fn}`, 'utf-8');
      return JSON.stringify({ filename: fn, content });
    } catch (err) {
      return JSON.stringify({ error: err.message });
    }
  }

  if (name === 'save_kb_file') {
    const fn = input.filename;
    const content = input.content;
    if (!fn || fn.includes('..') || fn.includes('/')) return JSON.stringify({ error: '非法文件名' });
    try {
      fs.writeFileSync(`${kbDir}/${fn}`, content);
      const stat = fs.statSync(`${kbDir}/${fn}`);
      const size = stat.size < 1024 ? `${stat.size}B` : stat.size < 1048576 ? `${(stat.size/1024).toFixed(1)}K` : `${(stat.size/1048576).toFixed(1)}M`;
      return JSON.stringify({ saved: fn, size });
    } catch (err) {
      return JSON.stringify({ error: err.message });
    }
  }

  return JSON.stringify({ error: `未知工具: ${name}` });
}

async function askKbAI(message, sessionId, files) {
  if (!sessionId) sessionId = `kb-${Date.now().toString(36)}`;
  if (!kbChatHistory.has(sessionId)) kbChatHistory.set(sessionId, []);
  const history = kbChatHistory.get(sessionId);

  // 首次对话且指定了文件，注入上下文提示
  let userMsg = message;
  if (history.length === 0 && files.length > 0) {
    userMsg = `[关联文件: ${files.join(', ')}]\n\n${message}`;
  }

  history.push({ role: 'user', content: userMsg });
  while (history.length > 40) history.shift();

  const messages = [...history];
  const intermediateTexts = [];

  for (let i = 0; i < 10; i++) {
    const body = {
      model: AI_MODEL,
      system: KB_SYSTEM_PROMPT,
      messages,
      max_tokens: 4000,
      tools: KB_TOOLS,
    };

    const result = await aiRequest(body);
    const content = result.content || [];
    const textBlocks = content.filter(b => b.type === 'text');
    const toolBlocks = content.filter(b => b.type === 'tool_use');

    if (textBlocks.length > 0) {
      const text = textBlocks.map(b => b.text).join('\n').trim();
      if (text) intermediateTexts.push(text);
    }

    if (toolBlocks.length === 0) {
      const finalText = textBlocks.map(b => b.text).join('\n') || '';
      if (intermediateTexts.length > 0 && finalText) intermediateTexts.pop();
      const allParts = [...intermediateTexts, finalText].filter(t => t.trim());
      const reply = allParts.join('\n\n') || '(无回复)';
      history.push({ role: 'assistant', content: reply.length > 2000 ? reply.substring(0, 2000) + '...' : reply });
      return { ok: true, reply, sessionId };
    }

    messages.push({ role: 'assistant', content });
    const toolResults = [];
    for (const tb of toolBlocks) {
      console.log(`📚 KB tool: ${tb.name}(${JSON.stringify(tb.input)})`);
      const toolResult = handleKbToolCall(tb);
      toolResults.push({ type: 'tool_result', tool_use_id: tb.id, content: toolResult });
    }
    messages.push({ role: 'user', content: toolResults });
  }

  return { ok: true, reply: '工具调用次数过多，请简化请求。', sessionId };
}

// ─── AI 回复清理 ────────────────────────────────────────────────────
function cleanAIResponse(text) {
  let cleaned = text;
  // Strip XML tool tags
  cleaned = cleaned.replace(/<(function_calls|antml:function_calls|antml:invoke|read_file|write_file|execute_command|search|list_dir|tool_call)[^>]*>[\s\S]*?<\/\1>/gi, '');
  cleaned = cleaned.replace(/<\/?(?:function_calls|antml:[a-z_]+|read_file|write_file|execute_command|search|list_dir|tool_call|invoke|parameter|path|command)[^>]*>/gi, '');
  // Strip Markdown formatting (DingTalk renders plain text only)
  cleaned = cleaned.replace(/\*\*([^*]+)\*\*/g, '$1');  // **bold** → bold
  cleaned = cleaned.replace(/\*([^*]+)\*/g, '$1');       // *italic* → italic
  cleaned = cleaned.replace(/`([^`]+)`/g, '$1');          // `code` → code
  cleaned = cleaned.replace(/```[\s\S]*?```/g, '');       // code blocks
  cleaned = cleaned.replace(/^#{1,6}\s+/gm, '');          // # headings
  cleaned = cleaned.replace(/^---+$/gm, '');              // --- dividers
  cleaned = cleaned.replace(/\n{3,}/g, '\n\n').trim();
  if (!cleaned || cleaned.length < 5) {
    return '抱歉，处理出错了。请重试。';
  }
  return cleaned;
}

function buildStartAgentAcceptedResponse(targetDevice, result) {
  if (result?.error) {
    return { ok: false, error: result.error };
  }
  return {
    ok: true,
    accepted: true,
    device: targetDevice?.name || targetDevice?.id || 'unknown',
    agent: {
      agent_id: result?.agent_id || null,
      tmux_session: result?.tmux_session || result?.session || null,
    },
    message: '已提交到 Mac 执行，稍后会出现在列表中',
  };
}

// ─── 防重复 ─────────────────────────────────────────────────────────
const processingMessages = new Set();

// ─── 心跳 HTTP 服务 + WebSocket ────────────────────────────────────
const heartbeatServer = http.createServer(async (req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

  if (req.method === 'OPTIONS') { res.writeHead(204); res.end(); return; }

  if (req.method === 'POST' && req.url === '/heartbeat') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', () => {
      try {
        const info = JSON.parse(body);
        registerDevice(info);
        console.log(`💓 心跳: ${info.name || info.id} (${info.ip || ''})`);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ status: 'ok', registered: devices.size }));
      } catch {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: '无效的 JSON' }));
      }
    });
    return;
  }

  // ─── CAM 通知接收端点 ───
  if (req.method === 'POST' && req.url === '/cam/notify') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const event = JSON.parse(body);
        const result = await handleCamNotify(event);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(result));
      } catch (err) {
        console.error(`❌ CAM notify 处理失败: ${err.message}`);
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── CAM pending 查询 ───
  if (req.method === 'GET' && req.url === '/cam/pending') {
    const pending = [];
    for (const [agentId, entry] of pendingCamNotifications) {
      pending.push({ agentId, eventType: entry.event.eventType, age: Date.now() - entry.timestamp });
    }
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ pending, count: pending.length }));
    return;
  }

  if (req.method === 'GET' && req.url === '/devices') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ devices: getAllDevices(), timestamp: Date.now() }));
    return;
  }

  // ─── Dashboard 登录验证 ───
  const DASH_PASSWORD = 'jsdtcj66AA';
  const DASH_TOKEN = require('crypto').createHash('sha256').update(DASH_PASSWORD).digest('hex').substring(0, 32);

  function checkDashAuth(req) {
    const cookies = (req.headers.cookie || '').split(';').reduce((m, c) => {
      const [k, ...v] = c.trim().split('=');
      if (k) m[k] = v.join('=');
      return m;
    }, {});
    return cookies['cam_token'] === DASH_TOKEN;
  }

  if (req.method === 'POST' && req.url === '/api/cam/login') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', () => {
      try {
        const { password } = JSON.parse(body);
        if (password === DASH_PASSWORD) {
          res.writeHead(200, {
            'Content-Type': 'application/json',
            'Set-Cookie': `cam_token=${DASH_TOKEN}; Path=/; HttpOnly; SameSite=Strict; Max-Age=7776000`,
          });
          res.end(JSON.stringify({ ok: true }));
        } else {
          res.writeHead(401, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: '密码错误' }));
        }
      } catch {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: '无效请求' }));
      }
    });
    return;
  }

  // Dashboard 及 API 鉴权（/dashboard 和 /api/cam/* 需要登录）
  if (req.url === '/dashboard' || req.url.startsWith('/api/cam/')) {
    if (!checkDashAuth(req)) {
      if (req.url === '/dashboard') {
        // 返回 dashboard 页面（前端自行判断未登录显示登录界面）
      } else {
        res.writeHead(401, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: '未登录' }));
        return;
      }
    }
  }

  // ─── Dashboard ───
  if (req.method === 'GET' && req.url === '/dashboard') {
    try {
      const fs = require('fs');
      const dashPath = require('path').join(__dirname, 'dashboard.html');
      const html = fs.readFileSync(dashPath, 'utf-8');
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      res.end(html);
    } catch (e) {
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end('Dashboard file not found');
    }
    return;
  }

  if (req.method === 'GET' && req.url === '/api/cam/state') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(camState));
    return;
  }

  if (req.method === 'GET' && req.url === '/api/cam/events') {
    res.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      'Connection': 'keep-alive',
    });
    res.write(`data: ${JSON.stringify({ type: 'state', ...camState })}\n\n`);
    sseClients.add(res);
    req.on('close', () => sseClients.delete(res));
    return;
  }

  if (req.method === 'POST' && req.url === '/api/cam/reply') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { agentId, input } = JSON.parse(body);
        if (!agentId || !input) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'agentId and input required' }));
          return;
        }
        const result = await sendDeviceRequest(null, {
          type: 'cam_reply',
          agentId,
          input,
        }, 10000);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, result }));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 终端输出捕获 ───
  if (req.method === 'GET' && req.url.startsWith('/api/cam/terminal')) {
    const params = new URL(req.url, 'http://localhost').searchParams;
    const session = params.get('session');
    const lines = parseInt(params.get('lines') || '80');
    if (!session) {
      res.writeHead(400, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: false, error: 'session required' }));
      return;
    }

    // 降级模式：从缓存返回
    if (deviceDegradedMode) {
      const cached = terminalSnapshotCache.get(session);
      if (cached) {
        const ageSec = Math.round((Date.now() - cached.timestamp) / 1000);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          ok: true,
          output: cached.output,
          cached: true,
          ageSec,
        }));
        return;
      }
      // 缓存没有，尝试实时获取（可能超时）
    }

    // 直连模式（或缓存未命中）：实时穿透
    try {
      const result = await sendDeviceRequest(null, {
        type: 'tmux_capture', session, lines, id: `cap-${Date.now()}`
      }, 30000);
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: true, output: result.output || '', truncated: !!result.truncated, error: result.error }));
    } catch (err) {
      // 降级模式下实时获取也失败，返回更友好的提示
      if (deviceDegradedMode) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: '终端缓存尚未就绪，请稍后刷新', cached: false }));
      } else {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    }
    return;
  }

  // ─── 向终端发送文本 ───
  if (req.method === 'POST' && req.url === '/api/cam/send') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { session, text, pressEnter } = JSON.parse(body);
        if (!session || text === undefined) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'session and text required' }));
          return;
        }
        const result = await sendDeviceRequest(null, {
          type: 'tmux_send', session, text, press_enter: pressEnter !== false,
          id: `send-${Date.now()}`
        }, 30000);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, result }));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 上传图片到 Mac ───
  if (req.method === 'POST' && req.url === '/api/cam/upload-image') {
    const chunks = [];
    req.on('data', c => chunks.push(c));
    req.on('end', async () => {
      try {
        const body = Buffer.concat(chunks);
        // 解析 multipart 或 JSON
        const contentType = req.headers['content-type'] || '';
        let fileName, base64Data;
        if (contentType.includes('application/json')) {
          const json = JSON.parse(body.toString());
          fileName = json.fileName;
          base64Data = json.data;
        } else {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'Content-Type must be application/json' }));
          return;
        }
        if (!fileName || !base64Data) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'fileName and data required' }));
          return;
        }
        // 安全检查
        const safeName = fileName.replace(/[^a-zA-Z0-9._-]/g, '_').substring(0, 100);
        const result = await sendDeviceRequest(null, {
          type: 'file_write', path: safeName, data: base64Data, encoding: 'base64',
          id: `upload-${Date.now()}`
        }, 30000);
        if (result?.error) {
          res.writeHead(500, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: result.error }));
        } else {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: true, path: result.path, size: result.size }));
        }
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 向终端发送原始按键 ───
  if (req.method === 'POST' && req.url === '/api/cam/send-key') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { session, key } = JSON.parse(body);
        if (!session || !key) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'session and key required' }));
          return;
        }
        const allowed = ['Escape', 'C-c', 'C-d', 'C-z', 'Enter', 'Up', 'Down'];
        if (!allowed.includes(key)) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: `key "${key}" not allowed. Allowed: ${allowed.join(', ')}` }));
          return;
        }
        const result = await sendDeviceRequest(null, {
          type: 'tmux_send_key', session, key,
          id: `key-${Date.now()}`
        }, 30000);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, result }));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 删除 Agent ───
  if (req.method === 'POST' && req.url === '/api/cam/agent/delete') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { agentId, killSession } = JSON.parse(body);
        if (!agentId) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'agentId required' }));
          return;
        }
        // 可选: kill tmux 会话
        if (killSession) {
          try {
            await sendDeviceRequest(null, {
              type: 'exec', command: `tmux kill-session -t ${agentId.replace(/[^a-zA-Z0-9_-]/g, '')}`,
              id: `kill-${Date.now()}`
            }, 5000);
          } catch {}
        }
        // 从 agents.json 移除
        await sendDeviceRequest(null, {
          type: 'exec',
          command: `python3 -c "
import json, os
p = os.path.expanduser('~/.config/code-agent-monitor/agents.json')
d = json.load(open(p))
d['agents'] = [a for a in d['agents'] if a.get('agent_id') != '${agentId.replace(/'/g, '')}' and a.get('tmux_session') != '${agentId.replace(/'/g, '')}']
json.dump(d, open(p, 'w'), indent=2)
print('removed')
"`,
          id: `rm-agent-${Date.now()}`
        }, 30000);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, deleted: agentId }));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 重启会话 ───
  if (req.method === 'POST' && req.url === '/api/cam/agent/restart') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { session } = JSON.parse(body);
        if (!session) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'session required' }));
          return;
        }
        const safeSession = session.replace(/[^a-zA-Z0-9_\-]/g, '');
        console.log(`🔄 Restart: ${safeSession}`);
        // 1. Ctrl+C 取消当前操作（如果 Claude 正在处理）
        await sendDeviceRequest(null, {
          type: 'tmux_send_key', session: safeSession, key: 'C-c',
          id: `restart-cc-${Date.now()}`
        }, 5000).catch(() => {});
        await new Promise(r => setTimeout(r, 800));
        // 2. 发 /exit 退出 Claude Code 进程
        await sendDeviceRequest(null, {
          type: 'tmux_send', session: safeSession, text: '/exit',
          id: `restart-exit-${Date.now()}`
        }, 5000).catch(() => {});
        // 3. 等 Claude Code 退出，回到 shell 提示符
        await new Promise(r => setTimeout(r, 3000));
        // 4. 发送重启命令（此时已在 shell 中）
        const restartCmd = 'claude --dangerously-skip-permissions --continue';
        await sendDeviceRequest(null, {
          type: 'tmux_send', session: safeSession, text: restartCmd,
          id: `restart-cmd-${Date.now()}`
        }, 30000);
        console.log(`✅ Restart sent: ${safeSession}`);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, session: safeSession }));
      } catch (err) {
        console.error(`❌ Restart failed: ${err.message}`);
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 分身（fork会话） ───
  if (req.method === 'POST' && req.url === '/api/cam/agent/clone') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { agentId, name } = JSON.parse(body);
        if (!agentId) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'agentId required' }));
          return;
        }
        const safeId = agentId.replace(/[^a-zA-Z0-9_\-\u4e00-\u9fff]/g, '');
        const nameFlag = name ? ` --name ${name.replace(/[^a-zA-Z0-9_-]/g, '').substring(0, 40)}` : '';
        const camCmd = `cam fork "${safeId}"${nameFlag} --json`;
        console.log(`🔀 Fork: ${camCmd}`);
        const result = await sendDeviceRequest(null, {
          type: 'exec', command: camCmd, id: `fork-${Date.now()}`
        }, 30000);
        console.log(`🔀 Fork result: exitCode=${result?.exitCode}, stdout=${(result?.stdout||'').length}c, stderr=${(result?.stderr||'').substring(0,200)}`);
        const output = (result?.stdout || result?.output || '').trim();
        const stderr = (result?.stderr || '').trim();
        try {
          const data = JSON.parse(output);
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: true, ...data }));
        } catch {
          // cam fork 可能输出非 JSON (stderr 混入)
          const combined = output || stderr;
          if (combined.includes('new_agent_id') || combined.includes('分叉成功')) {
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ ok: true, sessionName: name || 'fork', raw: combined }));
          } else {
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ ok: false, error: combined || 'fork 命令无输出', exitCode: result?.exitCode }));
          }
        }
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 启动新会话 ───
  if (req.method === 'POST' && req.url === '/api/cam/agent/start') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { cwd, prompt, name } = JSON.parse(body);
        if (!cwd || !prompt) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'cwd and prompt required' }));
          return;
        }
        const targetDevice = findExecutableDevice(null);
        if (!targetDevice) {
          res.writeHead(503, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: '没有可用的远程设备' }));
          return;
        }
        const caps = targetDevice.capabilities || [];
        if (!caps.includes('claude-code') && !caps.includes('tmux')) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: '设备不支持 Claude Code' }));
          return;
        }
        const result = await sendDeviceRequest(null, {
          type: 'tmux_start_claude',
          working_directory: cwd,
          prompt,
          session_name: name || '',
        }, 15000);
        const resp = buildStartAgentAcceptedResponse(targetDevice, result);
        res.writeHead(resp.ok ? 200 : 400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(resp));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── Agent 重命名 ───
  if (req.method === 'POST' && req.url === '/api/cam/agent/rename') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { agentId, newName } = JSON.parse(body);
        if (!agentId || !newName) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'agentId and newName required' }));
          return;
        }
        // 安全检查：newName 只允许字母数字和 -_
        const safeName = newName.replace(/[^a-zA-Z0-9_\-\u4e00-\u9fff]/g, '').substring(0, 40);
        if (!safeName) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: '名称无效' }));
          return;
        }
        // 在 Mac 上修改 agents.json
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
        const result = await sendDeviceRequest(null, {
          type: 'exec', command: pyCmd, id: `rename-${Date.now()}`
        }, 30000);
        const output = (result?.stdout || result?.output || '').trim();
        if (output.includes('not_found')) {
          res.writeHead(404, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: `Agent "${agentId}" 未找到` }));
        } else {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: true, oldName: agentId, newName: safeName }));
        }
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 钉钉通知 API（Agent 完成任务后主动汇报） ───
  if (req.method === 'POST' && req.url === '/api/cam/dingtalk-notify') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { message } = JSON.parse(body);
        if (!message) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'message required' }));
          return;
        }
        await sendDingtalkOtoMessage(message);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true }));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 钉钉聊天记录 ───
  if (req.method === 'GET' && req.url === '/api/cam/chat-history') {
    const result = {};
    for (const [k, v] of chatHistory) result[k] = v;
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ ok: true, conversations: result }));
    return;
  }

  // ─── 清除聊天记录 ───
  if (req.method === 'POST' && req.url === '/api/cam/chat-history/clear') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', () => {
      try {
        const { senderId } = body ? JSON.parse(body) : {};
        if (senderId) {
          chatHistory.delete(senderId);
        } else {
          chatHistory.clear();
        }
        saveChatHistory();
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, cleared: senderId || 'all' }));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── Dashboard 聊天 API ───
  if (req.method === 'POST' && req.url === '/api/cam/chat/send') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const { message } = JSON.parse(body);
        if (!message) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'message required' }));
          return;
        }
        const dashboardSenderId = '__dashboard__';
        // 处理清除上下文的特殊命令
        if (/^(新会话|清除记录|重置|reset|clear|\/new)$/i.test(message.trim())) {
          chatHistory.delete(dashboardSenderId);
          saveChatHistory();
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: true, reply: '✅ 会话已重置，上下文已清除。', cleared: true }));
          return;
        }
        const reply = await askAI(message, dashboardSenderId);
        saveChatHistory();
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, reply }));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── Dashboard 聊天历史 ───
  if (req.method === 'GET' && req.url === '/api/cam/chat/history') {
    const dashboardSenderId = '__dashboard__';
    const messages = chatHistory.get(dashboardSenderId) || [];
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ ok: true, messages }));
    return;
  }

  // ─── 知识库列表（本地 NAS 目录） ───
  if (req.method === 'GET' && req.url === '/api/cam/knowledge') {
    try {
      const fs = require('fs');
      const kbDir = '/kb';
      const entries = fs.readdirSync(kbDir);
      const files = entries.map(name => {
        try {
          const stat = fs.statSync(`${kbDir}/${name}`);
          if (!stat.isFile()) return null;
          const size = stat.size < 1024 ? `${stat.size}B` : stat.size < 1048576 ? `${(stat.size/1024).toFixed(1)}K` : `${(stat.size/1048576).toFixed(1)}M`;
          return { name, size, date: stat.mtime.toISOString().substring(0, 10) };
        } catch { return null; }
      }).filter(Boolean);
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: true, files }));
    } catch (err) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: true, files: [], note: '知识库目录未挂载' }));
    }
    return;
  }

  // ─── 知识库上传 ───
  if (req.method === 'POST' && req.url === '/api/cam/knowledge/upload') {
    const MAX_UPLOAD = 1024 * 1024; // 1MB
    const contentType = req.headers['content-type'] || '';
    if (!contentType.includes('multipart/form-data')) {
      res.writeHead(400, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: false, error: '需要 multipart/form-data 格式' }));
      return;
    }
    const boundaryMatch = contentType.match(/boundary=(.+)/);
    if (!boundaryMatch) {
      res.writeHead(400, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: false, error: '缺少 boundary' }));
      return;
    }
    const boundary = boundaryMatch[1];
    const chunks = [];
    let totalSize = 0;
    req.on('data', c => {
      totalSize += c.length;
      if (totalSize <= MAX_UPLOAD + 4096) chunks.push(c);
    });
    req.on('end', () => {
      try {
        if (totalSize > MAX_UPLOAD + 4096) {
          res.writeHead(413, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: '文件超过 1MB 限制' }));
          return;
        }
        const buf = Buffer.concat(chunks);
        const raw = buf.toString('binary');
        const parts = raw.split('--' + boundary).filter(p => p.trim() && p.trim() !== '--');
        let fileName = null, fileContent = null;
        for (const part of parts) {
          const headerEnd = part.indexOf('\r\n\r\n');
          if (headerEnd === -1) continue;
          const headers = part.substring(0, headerEnd);
          const body = part.substring(headerEnd + 4).replace(/\r\n$/, '');
          const nameMatch = headers.match(/filename="([^"]+)"/);
          if (nameMatch) {
            fileName = nameMatch[1].replace(/.*[\/\\]/, ''); // strip path
            fileContent = Buffer.from(body, 'binary');
            break;
          }
        }
        if (!fileName || !fileContent) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: '未找到上传文件' }));
          return;
        }
        if (fileName.includes('..') || fileName.includes('/')) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: '非法文件名' }));
          return;
        }
        const fs = require('fs');
        const filePath = `/kb/${fileName}`;
        fs.writeFileSync(filePath, fileContent);
        const stat = fs.statSync(filePath);
        const size = stat.size < 1024 ? `${stat.size}B` : stat.size < 1048576 ? `${(stat.size/1024).toFixed(1)}K` : `${(stat.size/1048576).toFixed(1)}M`;
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, file: { name: fileName, size, date: stat.mtime.toISOString().substring(0, 10) } }));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 知识库操作 ───
  if (req.method === 'POST' && req.url === '/api/cam/knowledge/action') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const fs = require('fs');
        const parsed = JSON.parse(body);
        const { action, file } = parsed;
        if (!file || !action) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'action and file required' }));
          return;
        }
        if (file.includes('..') || file.includes('/')) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: '非法文件名' }));
          return;
        }
        const filePath = `/kb/${file}`;

        if (action === 'read') {
          const content = fs.readFileSync(filePath, 'utf-8');
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: true, content }));
        } else if (action === 'create') {
          if (fs.existsSync(filePath)) {
            res.writeHead(409, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ ok: false, error: `文件已存在: ${file}` }));
            return;
          }
          fs.writeFileSync(filePath, parsed.content || '');
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: true, created: file }));
        } else if (action === 'save') {
          if (parsed.content === undefined) {
            res.writeHead(400, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ ok: false, error: '缺少 content 参数' }));
            return;
          }
          fs.writeFileSync(filePath, parsed.content);
          const stat = fs.statSync(filePath);
          const size = stat.size < 1024 ? `${stat.size}B` : stat.size < 1048576 ? `${(stat.size/1024).toFixed(1)}K` : `${(stat.size/1048576).toFixed(1)}M`;
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: true, saved: file, size }));
        } else if (action === 'delete') {
          fs.unlinkSync(filePath);
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: true, deleted: file }));
        } else if (action === 'execute') {
          const content = fs.readFileSync(filePath, 'utf-8');
          if (!content.trim()) {
            res.writeHead(400, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ ok: false, error: '文件内容为空' }));
            return;
          }
          const cwd = parsed.cwd || '/tmp';
          const sessionName = `kb-${file.replace(/\.[^.]+$/, '').replace(/[^a-zA-Z0-9-]/g, '-').substring(0, 20)}-${Date.now().toString(36)}`;
          const escaped = content.replace(/"/g, '\\"').replace(/\n/g, '\\n');
          const camCmd = `cam start --cwd "${cwd}" --name ${sessionName} "${escaped}"`;
          const execResult = await sendDeviceRequest(null, {
            type: 'exec', command: camCmd, id: `kb-exec-${Date.now()}`
          }, 60000);
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: true, session: sessionName, output: execResult.stdout || '' }));
        } else {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: `未知操作: ${action}` }));
        }
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  // ─── 知识挖掘 Chat API ───
  if (req.method === 'POST' && req.url === '/api/cam/knowledge/chat') {
    let body = '';
    req.on('data', c => (body += c));
    req.on('end', async () => {
      try {
        const parsed = JSON.parse(body);
        const { message, sessionId, files } = parsed;
        if (!message) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'message required' }));
          return;
        }
        const result = await askKbAI(message, sessionId || null, files || []);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(result));
      } catch (err) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: err.message }));
      }
    });
    return;
  }

  if (req.method === 'GET' && (req.url === '/health' || req.url === '/')) {
    const wsDevices = getAllDevices().filter(d => d.hasWs && d.online);
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      status: 'running',
      onlineDevices: getOnlineDevices().length,
      executableDevices: wsDevices.length,
      totalDevices: devices.size,
      uptime: process.uptime(),
    }));
    return;
  }

  res.writeHead(404); res.end('Not Found');
});

// ─── WebSocket 服务 ─────────────────────────────────────────────────
const wss = new WebSocketServer({ server: heartbeatServer, path: '/ws' });

wss.on('connection', (ws, req) => {
  const remoteIp = req.socket.remoteAddress;
  console.log(`🔗 WebSocket 连接: ${remoteIp}`);

  let deviceId = null;
  let authenticated = false;

  ws.on('message', (data) => {
    let msg;
    try { msg = JSON.parse(data); } catch { return; }

    if (msg.type === 'register') {
      if (msg.token !== WS_TOKEN) {
        console.log(`❌ WebSocket 认证失败: ${remoteIp}`);
        ws.send(JSON.stringify({ type: 'error', message: '认证失败' }));
        ws.close();
        return;
      }
      authenticated = true;
      deviceId = msg.device?.id || msg.device?.name || `ws-${remoteIp}`;
      registerDevice(msg.device || { id: deviceId, name: deviceId }, ws);
      deviceDegradedMode = !!msg.degraded;
      if (deviceDegradedMode) {
        console.log('⚠️ Mac bridge 处于降级模式，终端预览将使用缓存');
      }
      console.log(`✅ 设备注册: ${msg.device?.name || deviceId} (${remoteIp}) [WebSocket] ${deviceDegradedMode ? '[降级]' : '[直连]'}`);
      ws.send(JSON.stringify({ type: 'registered', deviceId }));
      return;
    }

    if (!authenticated) {
      ws.send(JSON.stringify({ type: 'error', message: '请先认证' }));
      return;
    }

    if (msg.type === 'heartbeat') {
      if (deviceId && devices.has(deviceId)) {
        const dev = devices.get(deviceId);
        dev.lastSeen = Date.now();
        if (msg.device) {
          Object.assign(dev, {
            ip: msg.device.ip || dev.ip,
            memory: msg.device.memory || dev.memory,
            disk: msg.device.disk || dev.disk,
            uptime: msg.device.uptime || dev.uptime,
          });
        }
      }
      ws.send(JSON.stringify({ type: 'pong' }));
      return;
    }

    // ─── Agent 完成事件（Mac bridge 检测到 busy→idle）───
    if (msg.type === 'agent_completed') {
      const agent = msg.agent || {};
      const id = agent.agent_id || agent.tmux_session || 'unknown';
      console.log(`🔔 Agent 完成检测: ${id} (busy→idle)`);
      broadcastSSE({ type: 'agent_completed', agent });
      handleAgentExit(agent, id).catch(err => {
        console.error(`⚠️ Agent 完成处理失败: ${err.message}`);
      });
      return;
    }

    // ─── 终端快照缓存（降级模式） ───
    if (msg.type === 'terminal_snapshots') {
      const { snapshots, timestamp } = msg;
      for (const [session, output] of Object.entries(snapshots || {})) {
        terminalSnapshotCache.set(session, { output, timestamp });
      }
      return;
    }

    // ─── CAM 状态推送 ───
    if (msg.type === 'cam_state_push') {
      if (!global._camPushCount) global._camPushCount = 0;
      global._camPushCount++;
      if (global._camPushCount % 12 === 1) {
        const statuses = (msg.agents || []).map(a => `${a.agent_id}:${a.status}`).join(', ');
        console.log(`📡 CAM 状态推送 #${global._camPushCount}: ${(msg.agents || []).length} agents [${statuses}]`);
      }
      const oldAgents = camState.agents || [];
      const newAgents = msg.agents || [];
      const newAgentIds = new Set(newAgents.map(a => a.agent_id || a.tmux_session));

      // 检测 agent 消失（session 退出）→ 统一走 handleAgentExit（AI 摘要 + 去重）
      // busy→idle（任务完成）由 mac-bridge 通过 agent_completed 事件处理，也走 handleAgentExit
      if (camState.timestamp === 0) {
        console.log('📡 首次收到 CAM 状态推送，跳过退出检测');
      } else {
        for (const old of oldAgents) {
          const oldId = old.agent_id || old.tmux_session;
          if (!newAgentIds.has(oldId)) {
            console.log(`📡 Agent 消失: ${oldId}，走统一退出处理`);
            handleAgentExit(old, oldId).catch(err => {
              console.error(`⚠️ Agent 退出处理失败: ${err.message}`);
            });
          }
        }
      }

      camState = {
        agents: newAgents,
        pending: msg.pending || { confirmations: [] },
        projectDirs: msg.projectDirs || camState.projectDirs || [],
        timestamp: msg.timestamp || Date.now(),
        macOnline: true,
        deviceId,
      };
      broadcastSSE({ type: 'state', ...camState });
      return;
    }

    // 通用结果响应（exec_result, tmux_result）
    if (msg.type === 'exec_result' || msg.type === 'tmux_result') {
      const pending = pendingCommands.get(msg.id);
      if (pending) {
        clearTimeout(pending.timer);
        pendingCommands.delete(msg.id);
        pending.resolve(msg.data || {
          stdout: msg.stdout || '',
          stderr: msg.stderr || '',
          exitCode: msg.exitCode ?? -1,
          device: deviceId,
        });
      }
      return;
    }
  });

  ws.on('close', () => {
    console.log(`⚡ WebSocket 断开: ${deviceId || remoteIp}`);
    if (deviceId && devices.has(deviceId)) {
      devices.get(deviceId).ws = null;
    }
    // 清理降级模式状态
    deviceDegradedMode = false;
    terminalSnapshotCache.clear();
    // 更新 Dashboard 状态
    if (deviceId && camState.deviceId === deviceId) {
      camState.macOnline = false;
      broadcastSSE({ type: 'state', ...camState });
    }
  });

  ws.on('error', (err) => {
    console.error(`❌ WebSocket 错误 (${deviceId || remoteIp}):`, err.message);
  });

  setTimeout(() => {
    if (!authenticated) {
      console.log(`⏰ WebSocket 认证超时: ${remoteIp}`);
      ws.close();
    }
  }, 30000);
});

// ─── 钉钉 Stream 客户端 ────────────────────────────────────────────
const client = new DWClient({ clientId: CLIENT_ID, clientSecret: CLIENT_SECRET });

client.registerCallbackListener(TOPIC_ROBOT, async (res) => {
  let data;
  try { data = JSON.parse(res.data); } catch { return res; }

  const content = (data.text?.content || '').trim();
  const senderNick = data.senderNick || '未知';
  const msgId = data.msgId || Date.now().toString();
  const senderId = data.senderId || 'default';
  const senderStaffId = data.senderStaffId || '';  // 钉钉 oTo API 需要的真实 userId
  const webhookUrl = data.sessionWebhook || '';
  const conversationType = data.conversationType || '1';  // "1"=单聊, "2"=群聊
  const isGroupChat = conversationType === '2';
  const atUsers = data.atUsers || [];
  const isAtBot = atUsers.some(u => u.dingtalkId === CLIENT_ID);

  console.log(`\n${'─'.repeat(50)}`);
  console.log(`📩 [${new Date().toLocaleTimeString()}] ${senderNick}: ${content}`);
  console.log(`   senderId=${senderId}, senderStaffId=${senderStaffId}, type=${isGroupChat ? '群聊' : '单聊'}, webhook=${webhookUrl ? 'yes' : 'no'}, atUsers=${JSON.stringify(atUsers.map(u => u.dingtalkId || u.staffId))}`);

  if (!content || processingMessages.has(msgId)) {
    client.socketCallBackResponse(res.headers?.messageId, { status: 'OK' });
    return res;
  }
  processingMessages.add(msgId);
  client.socketCallBackResponse(res.headers?.messageId, { status: 'OK' });

  // ─── @我 检测：群消息中有人@了我（不是@机器人），分析后私聊通知 ───
  if (isGroupChat && !isAtBot) {
    // 检查是否@了我（通过 staffId 或 dingtalkId 匹配 DINGTALK_USER_IDS）
    const isAtMe = atUsers.some(u =>
      DINGTALK_USER_IDS.includes(u.staffId) || DINGTALK_USER_IDS.includes(u.dingtalkId)
    );
    // 也排除自己发的消息
    const isFromMe = DINGTALK_USER_IDS.includes(senderStaffId);

    if (isAtMe && !isFromMe) {
      console.log(`🔔 检测到群消息@我: [${data.conversationTitle || '群聊'}] ${senderNick}: ${content.substring(0, 80)}`);
      try {
        const analysis = await analyzeAtMention({
          content,
          senderNick,
          conversationTitle: data.conversationTitle || '未知群',
          atUsers,
        });
        await sendDingtalkOtoMessage(analysis, DINGTALK_USER_IDS);
        console.log(`📤 @消息分析已私聊通知`);
      } catch (err) {
        console.error(`❌ @消息通知失败: ${err.message}`);
      }
      processingMessages.delete(msgId);
      return res;
    }

    // 群消息既没@机器人也没@我，忽略
    if (!isAtMe) {
      console.log(`   群消息未@机器人也未@我，忽略`);
      processingMessages.delete(msgId);
      return res;
    }
  }

  // 回复函数：群聊用 sessionWebhook 回群里，单聊用 oTo 私聊
  const replyToUser = async (text) => {
    if (isGroupChat && webhookUrl) {
      // 群聊：通过 sessionWebhook 回复到群
      console.log(`📤 回复群聊: webhook, text=${text.substring(0, 50)}...`);
      try {
        await sendViaWebhook(webhookUrl, text);
      } catch (err) {
        console.error(`❌ 群聊 webhook 发送失败: ${err.message}, 回退 oTo`);
        const replyUserIds = senderStaffId ? [senderStaffId] : DINGTALK_USER_IDS;
        await sendDingtalkOtoMessage(text, replyUserIds);
      }
    } else {
      // 单聊：通过 oTo API 私聊回复
      const replyUserIds = senderStaffId ? [senderStaffId] : DINGTALK_USER_IDS;
      console.log(`📤 回复单聊: userIds=${replyUserIds.join(',')}, text=${text.substring(0, 50)}...`);
      try {
        await sendDingtalkOtoMessage(text, replyUserIds);
      } catch (err) {
        console.error(`❌ oTo 发送失败: ${err.message}, 尝试 webhook 回退`);
        if (webhookUrl) {
          try {
            await sendViaWebhook(webhookUrl, text);
            console.log('✅ webhook 回退成功');
          } catch (err2) {
            console.error(`❌ webhook 回退也失败: ${err2.message}`);
          }
        }
      }
    }
  };

  try {
    // ─── CAM 回复检测（优先于 AI 对话） ───
    const camReply = findPendingCamReply(content);
    if (camReply) {
      console.log(`🔁 CAM 回复路由: agent=${camReply.agentId}, reply=${camReply.reply}`);
      try {
        await sendCamReplyToDevice(camReply.agentId, camReply.reply);
        await replyToUser(`✅ 已转发回复到 ${camReply.agentId}: ${camReply.reply}`);
      } catch (err) {
        await replyToUser(`❌ 回复转发失败: ${err.message}\n请在终端手动执行: cam reply "${camReply.reply}" --agent "${camReply.agentId}"`);
      }
    } else if (/^(新会话|清除记录|重置|reset|clear|\/new)$/i.test(content.trim())) {
      // ─── 清除会话上下文 ───
      chatHistory.delete(senderId);
      saveChatHistory();
      console.log(`🧹 已清除会话: ${senderId}`);
      await replyToUser('✅ 会话已重置，上下文已清除。');
    } else if (isDeviceCommand(content)) {
      console.log('📡 设备状态查询');
      const reply = formatDeviceList();
      await replyToUser(reply);
      console.log('📤 已回复设备列表');
    } else {
      await replyToUser('⏳ 正在处理中...');
      console.log('🤖 正在调用 AI...');
      const rawReply = await askAI(content, senderId);
      const reply = cleanAIResponse(rawReply);
      console.log(`✅ 回复 (${reply.length} 字符)`);
      await replyToUser(reply);
      console.log('📤 已回复');
    }
  } catch (error) {
    console.error('❌ 错误:', error.message);
    try { await replyToUser(`⚠️ 出错: ${error.message}`); } catch {}
  } finally {
    processingMessages.delete(msgId);
  }
  return res;
});

function sendViaWebhook(url, content) {
  return new Promise((resolve, reject) => {
    const body = JSON.stringify({ msgtype: 'text', text: { content } });
    const transport = url.startsWith('https') ? https : http;
    const req = transport.request(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) },
    }, (res) => {
      let d = '';
      res.on('data', c => (d += c));
      res.on('end', () => {
        try { const r = JSON.parse(d); r.errcode === 0 ? resolve(r) : reject(new Error(r.errmsg)); } catch { resolve(d); }
      });
    });
    req.on('error', reject);
    req.write(body); req.end();
  });
}

// ─── 启动 ───────────────────────────────────────────────────────────
heartbeatServer.listen(HEARTBEAT_PORT, '0.0.0.0', () => {
  console.log('');
  console.log('🦞 ═══════════════════════════════════════════════');
  console.log('   钉钉 AI 助手 + 设备监控 + CAM 消息中枢');
  console.log('   ─────────────────────────────────────────────');
  console.log(`   Client ID:    ${CLIENT_ID}`);
  console.log(`   AI Model:     ${AI_MODEL}`);
  console.log(`   心跳端口:     ${HEARTBEAT_PORT}`);
  console.log(`   WebSocket:    ws://0.0.0.0:${HEARTBEAT_PORT}/ws`);
  console.log(`   CAM Notify:   POST :${HEARTBEAT_PORT}/cam/notify`);
  console.log(`   钉钉用户:     ${DINGTALK_USER_IDS.join(', ')}`);
  console.log(`   设备超时:     ${DEVICE_TIMEOUT}秒`);
  console.log('   ─────────────────────────────────────────────');
  console.log('   AI 工具: execute_command / list_devices');
  console.log('            start_claude_code / send_to_tmux');
  console.log('            check_tmux_output / list_tmux_sessions');
  console.log('   CAM:     POST /cam/notify (通知接收)');
  console.log('            GET  /cam/pending (待处理列表)');
  console.log('═══════════════════════════════════════════════════');
  console.log('');

  client.connect();
  console.log('✅ 钉钉 Stream 已连接');
  console.log(`✅ HTTP + WebSocket 服务监听 0.0.0.0:${HEARTBEAT_PORT}`);
  console.log('等待消息...\n');
});
