# 前端 Dockerfile - 开发模式
FROM node:18-slim

WORKDIR /app

# 安装 pnpm
RUN npm install -g pnpm

# 复制依赖文件
COPY package.json pnpm-lock.yaml* ./

# 安装依赖
RUN pnpm install --frozen-lockfile

# 复制源码
COPY . .

# 暴露端口
EXPOSE 3000

# 使用开发模式启动（利用 Vite proxy 功能）
CMD ["pnpm", "dev", "--host", "0.0.0.0"]
