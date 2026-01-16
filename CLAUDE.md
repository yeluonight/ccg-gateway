# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

CCG Gateway 是一个支持 Claude Code、Codex、Gemini 的 API 网关转发项目，提供统一的代理入口和管理界面。

## 架构

前后端分离架构：
- **后端**：Python 3.10+ / FastAPI / SQLAlchemy / SQLite
- **前端**：Vue 3 / TypeScript / Element Plus / Pinia / Vite

```
backend/
├── app/
│   ├── api/           # API 路由 (admin.py 管理接口, proxy.py 代理转发)
│   ├── core/          # 核心配置 (config, database, uptime)
│   ├── models/        # SQLAlchemy 模型
│   ├── schemas/       # Pydantic 模型
│   └── services/      # 业务逻辑层
└── data/              # SQLite 数据库

frontend/src/
├── api/               # Axios API 封装
├── views/             # 页面组件
├── stores/            # Pinia 状态管理
└── types/             # TypeScript 类型
```

## 常用命令

### 一键启动
```bash
# Windows
start.bat

# Linux/macOS
./start.sh
```

### 手动启动
```bash
# 后端
cd backend
uv run uvicorn app.main:app --host 127.0.0.1 --port 7788 --reload

# 前端
cd frontend
pnpm install
pnpm dev      # 开发
pnpm build    # 构建
```

## 端口配置

通过 `.env` 文件配置：
- `GATEWAY_PORT`：网关端口（默认 7788）
- `UI_PORT`：UI 端口（默认 3000）

## API 结构

- `/{path}` - 代理转发（所有 HTTP 方法）
- `/admin/v1/*` - 管理接口
- `/health` - 健康检查

## 数据模型

核心模型位于 `backend/app/models/`：
- `Provider` - 服务商配置
- `ProviderModelMap` - 模型名称映射
- `GatewaySettings` / `TimeoutSettings` - 网关配置
- `CliSettings` - CLI 配置 (claude_code/codex/gemini)
- `UsageDaily` / `RequestLog` / `SystemLog` - 统计与日志
- `WebdavSettings` - WebDAV 备份配置

## 包管理器

- 后端：`uv`
- 前端：`pnpm`
