# Stage 1: 构建前端
FROM node:20-slim AS frontend-builder
WORKDIR /build
RUN npm install -g pnpm
COPY frontend/package.json frontend/pnpm-lock.yaml* ./
RUN pnpm install --frozen-lockfile
COPY frontend/ .
RUN pnpm build

# Stage 2: 后端 + 前端静态文件
FROM python:3.13-slim
WORKDIR /app

# 安装 uv 并安装依赖
RUN pip install --no-cache-dir uv
COPY backend/pyproject.toml backend/uv.lock* ./
RUN uv sync --no-dev

# 复制后端代码
COPY backend/ .

# 复制前端构建产物
COPY --from=frontend-builder /build/dist /app/frontend/dist

# 创建数据目录
RUN mkdir -p /data

EXPOSE 7788

ENV FRONTEND_DIST=/app/frontend/dist

CMD ["uv", "run", "uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "7788"]
