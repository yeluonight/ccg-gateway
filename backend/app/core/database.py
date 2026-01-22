from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession, async_sessionmaker
from sqlalchemy.orm import DeclarativeBase
from sqlalchemy import text

from app.core.config import DATA_DIR


class Base(DeclarativeBase):
    """配置数据库的 Base 类"""
    pass


class LogBase(DeclarativeBase):
    """日志数据库的 Base 类"""
    pass


# 配置数据库引擎
engine = create_async_engine(
    f"sqlite+aiosqlite:///{DATA_DIR}/ccg_gateway.db",
    echo=False,
    connect_args={
        "check_same_thread": False,
        "timeout": 30
    },
    pool_pre_ping=True,
    pool_recycle=3600,
)

# 日志数据库引擎
log_engine = create_async_engine(
    f"sqlite+aiosqlite:///{DATA_DIR}/ccg_logs.db",
    echo=False,
    connect_args={
        "check_same_thread": False,
        "timeout": 30
    },
    pool_pre_ping=True,
    pool_recycle=3600,
)

# 配置数据库会话工厂
async_session_maker = async_sessionmaker(
    engine,
    class_=AsyncSession,
    expire_on_commit=False
)

# 日志数据库会话工厂
async_log_session_maker = async_sessionmaker(
    log_engine,
    class_=AsyncSession,
    expire_on_commit=False
)


async def get_db():
    async with async_session_maker() as session:
        try:
            yield session
        finally:
            await session.close()


async def get_log_db():
    async with async_log_session_maker() as session:
        try:
            yield session
        finally:
            await session.close()


async def _ensure_auto_vacuum(conn):
    """确保数据库启用 auto_vacuum = FULL"""
    result = await conn.execute(text("PRAGMA auto_vacuum"))
    current_mode = result.scalar()
    if current_mode != 1:  # 1 = FULL
        await conn.execute(text("PRAGMA auto_vacuum = FULL"))
        await conn.execute(text("VACUUM"))


async def init_db():
    async with engine.begin() as conn:
        await _ensure_auto_vacuum(conn)
        await conn.run_sync(Base.metadata.create_all)


async def init_log_db():
    async with log_engine.begin() as conn:
        await _ensure_auto_vacuum(conn)
        await conn.run_sync(LogBase.metadata.create_all)


async def close_db():
    await engine.dispose()
    await log_engine.dispose()
