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
        // 验证参数完整性
        if (!params.agent_id || !params.message) {
          const errorMsg = `Missing required params: agent_id=${params.agent_id}, message=${params.message}`;
          api.logger.error("cam_agent_send validation failed", { params, errorMsg });
          return { content: [{ type: "text", text: JSON.stringify({ error: true, message: errorMsg }) }] };
        }
        const result = await callCamMcp("agent_send", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_send failed", { error: error.message, params });
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

  // === Team Discovery ===

  api.registerTool({
    name: "cam_team_list",
    description: "列出所有 Agent Teams / List all agent teams",
    parameters: Type.Object({}),
    async execute() {
      try {
        const result = await callCamMcp("team_list", {});
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_list failed", { error });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_team_members",
    description: "列出 Team 的所有成员 / List all members of a team",
    parameters: Type.Object({
      team_name: Type.String({ description: "Team 名称" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_members", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_members failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // === Team Bridge ===

  api.registerTool({
    name: "cam_team_create",
    description: "创建新的 Agent Team / Create a new agent team",
    parameters: Type.Object({
      name: Type.String({ description: "Team 名称" }),
      description: Type.String({ description: "Team 描述" }),
      project_path: Type.String({ description: "项目路径" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_create", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_create failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_team_delete",
    description: "删除 Agent Team / Delete an agent team",
    parameters: Type.Object({
      name: Type.String({ description: "Team 名称" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_delete", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_delete failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_team_status",
    description: "获取 Team 状态 / Get team status",
    parameters: Type.Object({
      name: Type.String({ description: "Team 名称" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_status", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_status failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_inbox_read",
    description: "读取 Team 成员的收件箱 / Read a team member's inbox",
    parameters: Type.Object({
      team: Type.String({ description: "Team 名称" }),
      member: Type.String({ description: "成员名称" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("inbox_read", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_inbox_read failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_inbox_send",
    description: "向 Team 成员发送消息 / Send a message to a team member",
    parameters: Type.Object({
      team: Type.String({ description: "Team 名称" }),
      member: Type.String({ description: "成员名称" }),
      message: Type.String({ description: "消息内容" }),
      from: Type.Optional(Type.String({ description: "发送者（默认 'user'）" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("inbox_send", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_inbox_send failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_team_pending_requests",
    description: "查看 Team 中待处理的请求 / List pending requests in a team",
    parameters: Type.Object({
      team: Type.Optional(Type.String({ description: "Team 名称（可选，不填则查看所有）" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_pending_requests", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_pending_requests failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // === Team Orchestration ===

  api.registerTool({
    name: "cam_team_spawn_agent",
    description: "在 Team 中启动新 Agent / Spawn a new agent in a team",
    parameters: Type.Object({
      team: Type.String({ description: "Team 名称" }),
      name: Type.String({ description: "Agent 名称" }),
      agent_type: Type.Optional(Type.String({ description: "Agent 类型: claude/opencode/codex" })),
      initial_prompt: Type.String({ description: "Agent 初始提示词" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_spawn_agent", params, 45000);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_spawn_agent failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_team_progress",
    description: "查看 Team 整体进度 / View team progress",
    parameters: Type.Object({
      team: Type.String({ description: "Team 名称" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_progress", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_progress failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_team_shutdown",
    description: "关闭 Team 及其所有 Agent / Shutdown a team and all its agents",
    parameters: Type.Object({
      team: Type.String({ description: "Team 名称" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_shutdown", params, 30000);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_shutdown failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_team_orchestrate",
    description: "自动编排 Team 执行任务 / Auto-orchestrate a team to execute a task",
    parameters: Type.Object({
      task_desc: Type.String({ description: "任务描述" }),
      project: Type.Optional(Type.String({ description: "项目路径（可选）" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_orchestrate", params, 60000);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_orchestrate failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_team_assign_task",
    description: "向 Team 成员分配任务 / Assign a task to a team member",
    parameters: Type.Object({
      team: Type.String({ description: "Team 名称" }),
      member: Type.String({ description: "成员名称" }),
      task: Type.String({ description: "任务描述" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("team_assign_task", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_team_assign_task failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // === Task Management ===

  api.registerTool({
    name: "cam_task_list",
    description: "列出 Team 的任务列表 / List tasks for a team",
    parameters: Type.Object({
      team_name: Type.String({ description: "Team 名称" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("task_list", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_task_list failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_task_get",
    description: "获取任务详情 / Get task details",
    parameters: Type.Object({
      team_name: Type.String({ description: "Team 名称" }),
      task_id: Type.String({ description: "任务 ID" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("task_get", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_task_get failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_task_update",
    description: "更新任务状态 / Update task status",
    parameters: Type.Object({
      team_name: Type.String({ description: "Team 名称" }),
      task_id: Type.String({ description: "任务 ID" }),
      status: Type.String({ description: "新状态 (pending/in_progress/completed/deleted)" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("task_update", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_task_update failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // === Reply Management ===

  api.registerTool({
    name: "cam_get_pending_confirmations",
    description: "查看所有待处理的确认请求 / List all pending confirmation requests",
    parameters: Type.Object({}),
    async execute() {
      try {
        const result = await callCamMcp("get_pending_confirmations", {});
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_get_pending_confirmations failed", { error });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_reply_pending",
    description: "回复待处理的确认请求 / Reply to a pending confirmation request",
    parameters: Type.Object({
      reply: Type.String({ description: "回复内容（如 'y', 'n', 或具体回复）" }),
      target: Type.Optional(Type.String({ description: "目标 agent（可选，支持通配符如 'cam-*'）" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("reply_pending", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_reply_pending failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_handle_user_reply",
    description: "处理用户回复（由通知系统调用）/ Handle a user reply from notification system",
    parameters: Type.Object({
      reply: Type.String({ description: "用户回复内容" }),
      context: Type.Optional(Type.String({ description: "回复上下文（如 agent_id 或 session_id）" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("handle_user_reply", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_handle_user_reply failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // === Agent Info ===

  api.registerTool({
    name: "cam_get_agent_info",
    description: "通过 PID 获取 Agent 详细信息 / Get agent info by PID",
    parameters: Type.Object({
      pid: Type.Number({ description: "进程 ID" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("get_agent_info", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_get_agent_info failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_get_session_info",
    description: "获取会话详细信息 / Get session info by session ID",
    parameters: Type.Object({
      session_id: Type.String({ description: "会话 ID" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("get_session_info", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_get_session_info failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_agent_by_session_id",
    description: "通过会话 ID 查找 Agent / Find agent by session ID",
    parameters: Type.Object({
      session_id: Type.String({ description: "会话 ID" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_by_session_id", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_by_session_id failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });
}
