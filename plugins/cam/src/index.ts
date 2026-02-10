import { Type } from "@sinclair/typebox";
import { spawn } from "child_process";
import { fileURLToPath } from "url";
import { dirname, join } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const CAM_BIN = join(__dirname, "..", "bin", "cam");

// 通用 MCP 调用函数
async function callCamMcp(toolName: string, args: object, timeoutMs: number = 10000): Promise<object> {
  return new Promise((resolve, reject) => {
    const request = JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { name: toolName, arguments: args }
    });

    const proc = spawn(CAM_BIN, ["serve"], { timeout: timeoutMs });
    let stdout = "";
    let stderr = "";

    proc.stdout.on("data", (data) => stdout += data);
    proc.stderr.on("data", (data) => stderr += data);

    proc.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`CAM exited with code ${code}: ${stderr}`));
        return;
      }
      try {
        const response = JSON.parse(stdout);
        if (response.error) {
          reject(new Error(response.error.message));
        } else {
          resolve(response.result);
        }
      } catch (e) {
        reject(new Error(`Invalid JSON response: ${stdout}`));
      }
    });

    proc.stdin.write(request);
    proc.stdin.end();
  });
}

export default function (api) {
  // Agent 生命周期管理
  api.registerTool({
    name: "cam_agent_start",
    description: "启动新的 Claude Code agent。必须提供 project_path。",
    parameters: Type.Object({
      project_path: Type.String({ description: "项目目录路径（必填）" }),
      agent_type: Type.Optional(Type.String({ description: "代理类型: claude/opencode/codex，默认 claude" })),
      prompt: Type.Optional(Type.String({ description: "初始提示词" })),
      agent_id: Type.Optional(Type.String({ description: "自定义 agent ID（可选，默认自动生成）" })),
      tmux_session: Type.Optional(Type.String({ description: "自定义 tmux session 名称（可选，默认使用 agent_id）" })),
    }),
    async execute(_id, params) {
      try {
        // 生成默认的 agent_id 和 tmux_session（如果未提供）
        const timestamp = Math.floor(Date.now() / 1000);
        const agentId = params.agent_id || `cam-${timestamp}`;
        const tmuxSession = params.tmux_session || agentId;

        // agent_start 需要更长的超时时间，因为要等待 Claude Code 就绪（最多 30 秒）
        const result = await callCamMcp("agent_start", {
          agent_type: params.agent_type || "claude",
          project_path: params.project_path,
          initial_prompt: params.prompt,  // MCP 工具期望 initial_prompt
          agent_id: agentId,
          tmux_session: tmuxSession,
        }, 45000);  // 45 秒超时
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_start failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_agent_stop",
    description: "停止一个运行中的 agent",
    parameters: Type.Object({
      agent_id: Type.String({ description: "Agent ID (如 cam-xxxxxxxx)" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_stop", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_stop failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_agent_list",
    description: "列出所有 CAM 管理的运行中 agent",
    parameters: Type.Object({}),
    async execute() {
      try {
        const result = await callCamMcp("agent_list", {});
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_list failed", { error });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // Agent 交互
  api.registerTool({
    name: "cam_agent_send",
    description: "向 agent 发送消息/输入（用于确认、拒绝或发送指令）",
    parameters: Type.Object({
      agent_id: Type.String({ description: "Agent ID" }),
      message: Type.String({ description: "要发送的消息" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_send", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_send failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_agent_status",
    description: "获取 agent 的结构化状态（是否等待输入、最近工具调用等）。用于诊断 agent 状态。",
    parameters: Type.Object({
      agent_id: Type.String({ description: "Agent ID" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_status", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_status failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_agent_logs",
    description: "获取 agent 的终端输出日志。只用于查看，不发送任何输入。注意：终端中显示的百分比（如 23%）是 context window 占用率，不是任务进度。占用率高表示会话 context 快满了，与任务是否完成无关。",
    parameters: Type.Object({
      agent_id: Type.String({ description: "Agent ID" }),
      lines: Type.Optional(Type.Number({ description: "返回行数，默认 50" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_logs", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_logs failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // 会话管理
  api.registerTool({
    name: "cam_list_sessions",
    description: "列出历史 Claude Code 会话（包括 CAM 启动的和 Mac 直接打开的）。可用于恢复之前的工作。",
    parameters: Type.Object({
      project_path: Type.Optional(Type.String({ description: "按项目路径过滤（模糊匹配）" })),
      days: Type.Optional(Type.Number({ description: "只返回最近 N 天的会话" })),
      limit: Type.Optional(Type.Number({ description: "限制返回数量" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("list_sessions", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_list_sessions failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_resume_session",
    description: "恢复一个历史会话到 tmux，并注册到 CAM 管理。支持恢复任意 Claude Code 会话。",
    parameters: Type.Object({
      session_id: Type.String({ description: "会话 ID" }),
    }),
    async execute(_id, params) {
      try {
        // resume_session 也需要等待 Claude Code 就绪
        const result = await callCamMcp("resume_session", params, 45000);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_resume_session failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // 进程管理（低级）
  api.registerTool({
    name: "cam_list_agents",
    description: "列出系统中所有 Claude Code 进程（包括非 CAM 管理的）",
    parameters: Type.Object({}),
    async execute() {
      try {
        const result = await callCamMcp("list_agents", {});
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_list_agents failed", { error });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_kill_agent",
    description: "终止一个 agent 进程（通过 PID）",
    parameters: Type.Object({
      pid: Type.Number({ description: "进程 ID" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("kill_agent", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_kill_agent failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_send_input",
    description: "向 tmux 会话发送原始输入",
    parameters: Type.Object({
      tmux_session: Type.String({ description: "tmux 会话名称" }),
      input: Type.String({ description: "要发送的输入" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("send_input", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_send_input failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });
}
