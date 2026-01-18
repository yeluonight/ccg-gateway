# 后端 Dockerfile
FROM python:3.10-slim

WORKDIR /app

# 安装 uv
RUN pip install --no-cache-dir uv

# 复制依赖文件
COPY pyproject.toml ./

# 安装依赖
RUN uv sync --no-dev

# 复制应用代码
COPY . .

# 创建数据目录
RUN mkdir -p /data

# 暴露端口
EXPOSE 7788

# 启动命令
CMD ["uv", "run", "uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "7788"]
