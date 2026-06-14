# CAB — Agent 开发约定

本仓库日常开发环境在本机**全局唯一**：固定端口、固定启动命令、每类进程至多一个实例。AI Agent 与贡献者必须遵守。

## 全局唯一（最高原则）

**整台机器上**同时只能存在一套 CAB 开发环境：

| 资源 | 全局唯一 | 固定值 |
|------|----------|--------|
| 前端 dev 进程 | **1** | `npm run dev` → `127.0.0.1:5173` |
| 后端 watch 进程 | **1** | `npm run dev:server` → `127.0.0.1:3125` |
| Gateway 端口 | **1** | **3125**（不可改） |
| 前端 dev 端口 | **1** | **5173**（不可改） |

含义：

- **不论**开几个终端、几个 Cursor 会话、几个仓库目录——5173 与 3125 上**各只能有一个**监听进程。
- **禁止** Tauri、`cab-server.exe`、release/debug 二进制与 `npm run dev:server` **同时**占用 3125。
- **禁止** 第二个 `npm run dev`、第二个 Vite 实例占用 5173。
- **禁止** 换端口绕过占用；**禁止** 不 kill 旧进程直接再起第二个实例。

## 端口占用：kill 后再起

启动 `npm run dev` / `npm run dev:server` 时，若 5173 或 3125 **已被占用**（含僵尸 `cab-server.exe`、旧 Vite、上次 crash 残留）：

1. **Kill 占用进程**（不换端口）  
   ```powershell
   powershell -File scripts/kill-dev-ports.ps1          # 5173 + 3125
   powershell -File scripts/kill-dev-ports.ps1 -Backend # 仅 3125
   ```
   或手动：`netstat -ano | findstr ":3125.*LISTENING"` → `Stop-Process -Id <PID> -Force`

2. **确认端口已释放**（`netstat` 无 LISTENING）

3. **再用唯一允许的命令启动**（仍各 1 个实例）  
   - 前端：`npm run dev`  
   - 后端：`npm run dev:server`

优先在原 dev 终端 `Ctrl+C` 正常退出；只有端口仍被占用或 watch 已挂、起不来时，才用 kill 脚本清理后再起。

## 启动方式（唯一允许）

仓库根目录 **两个终端**（共两个进程，全局仅此一套）：

### 1. 前端 — 热更新

```bash
npm run dev
```

- 地址：`http://127.0.0.1:5173`（`strictPort: true`，端口冲突 → kill 后再起，禁止改端口）

### 2. 后端 — watch 模式

```bash
npm run dev:server
```

- Gateway / API：`http://127.0.0.1:3125`
- 首次需安装：`cargo install cargo-watch`

## 禁止的启动方式

| 禁止 | 原因 |
|------|------|
| `cargo run -p cab-server` | 无 watch；易与 watch 实例冲突 |
| `npm run dev:server:once` | 非 watch |
| `target/**/cab-server.exe` 直接运行 | 非 watch；应 kill 后用 `dev:server` |
| 端口占用时改端口或叠第二个实例 | 破坏全局唯一 |
| `npm run tauri:dev` / `npm run tauri:start` | 与约定 dev 流程冲突 |
| `cargo build --release` 后单独起 server | 发布流程，非日常 dev |
| 修改 5173 / 3125 | 端口固定 |

## 验证

- 前端：`http://127.0.0.1:5173`
- 后端：`curl -H "Authorization: Bearer <gateway_key>" http://127.0.0.1:3125/v1/models`
- 检查占用：`netstat -ano | findstr "5173 3125"`

## 数据与配置

- 运行时状态：`~/.cab/`（`settings.json`、`state.json`、日志等）
- `gateway_port` 保持 **3125**
- Agent CLI 的 `ANTHROPIC_BASE_URL` 必须是 `http://localhost:3125`

## 真实无头测试（Claude Code）

集成验证**必须**走真实 Agent CLI 无头模式，**禁止**仅用 curl/mock 代替。

**前置**：3125 上为全局唯一的 `npm run dev:server`。若未监听 → `kill-dev-ports.ps1 -Backend` → `npm run dev:server`。

```powershell
powershell -File scripts/test-cc-headless.ps1
```

脚本检查 3125、配置 `claude-code` auto、执行 `claude -p`、核对网关日志。测试前若 3125 被僵尸进程占用，先 kill 再起 backend，**禁止**另起 release exe。

## 修改完成：自测后再汇报（强制）

**任何代码/配置改动完成后，Agent 必须先在本机跑完验证，再向用户汇报结果。** 禁止「改完就说好了」、禁止把未验证的假设当结论。

### 流程

1. **清理环境**（避免叠进程、假失败）  
   ```powershell
   powershell -File scripts/kill-dev-ports.ps1
   Get-Process claude,cab-server,cargo-watch -ErrorAction SilentlyContinue | Stop-Process -Force
   ```

2. **启动唯一 dev 套**（各 1 个进程）  
   - 终端 A：`npm run dev:server`（等到 catalog sync 完成、`3125` LISTENING）  
   - 终端 B：`npm run dev`（`5173` LISTENING）

3. **同步 token**（CC 401 的常见原因）  
   - `gateway_key` 来自 `~/.cab/settings.json`  
   - 改 key 或重启后：`PUT /api/settings` 空 body 触发 agent sync，或确认 `~/.claude/settings.json` 里 `ANTHROPIC_AUTH_TOKEN` 与 `gateway_key` 一致

4. **最小验证清单**（全部通过才算完成）  

   | 步骤 | 命令/检查 | 期望 |
   |------|-----------|------|
   | Provider | `GET /api/providers` | 目标 provider `enabled` 且有 key |
   | 路由 | `POST /api/routing/explain` body `{"agent":"claude-code","model":"auto"}` | `resolved` 非空 |
   | 网关 | `POST /v1/messages`（`x-api-key: <gateway_key>`） | HTTP 200 |
   | 前端 | `GET http://127.0.0.1:5173` | HTTP 200 |
   | CC 无头 | 见下方 | 输出含 `CAB ok`，进程在超时内退出 |
   | 配置 | 读 `~/.cab/settings.json` | `providers` 未被清空 |

5. **CC 无头测试**（必须带硬超时，禁止无限挂后台）  
   ```powershell
   $key = (Get-Content "$env:USERPROFILE\.cab\settings.json" | ConvertFrom-Json).gateway_key
   $env:ANTHROPIC_BASE_URL = "http://127.0.0.1:3125"
   $env:ANTHROPIC_AUTH_TOKEN = $key
   $env:CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY = "1"
   $job = Start-Job { param($c) & $c -p "Reply with exactly: CAB ok" --model "claude/cab/auto" --max-turns 1 2>&1 | Out-String } -ArgumentList "$env:USERPROFILE\.local\bin\claude.exe"
   Wait-Job $job -Timeout 120
   if ((Get-Job $job.Id).State -eq 'Running') { Stop-Job $job; Get-Process claude -EA SilentlyContinue | Stop-Process -Force; throw 'CC test timeout' }
   Receive-Job $job; Remove-Job $job -Force
   ```
   或：`powershell -File scripts/test-cc-headless.ps1 -TimeoutSec 120`（超时必须 kill `claude.exe`）。

6. **收尾**  
   - 测试用的 `claude.exe` 必须已退出；不得遗留多个 `cargo watch` / `test-cc-headless.ps1`  
   - 若只改后端：可只留 `dev:server`；若用户需要 UI 验证：前后端都留  
   - 汇报时附上：端口状态、路由结果、CC 输出、失败时的 gateway log（`GET /api/logs?per_page=3`）

### 禁止

- 修改 `settings.json` 相关逻辑后不验证 provider key 是否仍在  
- 把超时的 CC 测试丢后台不 kill  
- 多次 `npm run dev:server` 不 kill 旧进程  
- 仅用 curl 跳过 CC 无头（网关 curl 只是清单其中一步，不能替代 CC）

### 向用户汇报格式

- **通过**：服务地址 + 关键测试结果（路由、CC 输出）  
- **失败**：失败步骤 + 日志/状态码 + 已尝试的修复；不要只写「应该可以了」

