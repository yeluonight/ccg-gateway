import shutil
import time
from pathlib import Path
from datetime import datetime

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.core.config import DATA_DIR
from app.core.database import engine
from app.models.models import WebdavSettings

# 只备份配置数据库，日志数据库 (ccg_logs.db) 不参与备份
DB_FILE = DATA_DIR / "ccg_gateway.db"
BACKUP_DIR = DATA_DIR / "backups"
WEBDAV_REMOTE_PATH = "/ccg-gateway-backup"


async def get_webdav_settings(db: AsyncSession) -> WebdavSettings:
    result = await db.execute(select(WebdavSettings).where(WebdavSettings.id == 1))
    settings = result.scalar_one_or_none()
    if not settings:
        settings = WebdavSettings(id=1, url="", username="", password="", updated_at=int(time.time()))
        db.add(settings)
        await db.commit()
        await db.refresh(settings)
    return settings


async def update_webdav_settings(db: AsyncSession, url: str = None, username: str = None, password: str = None) -> WebdavSettings:
    settings = await get_webdav_settings(db)
    if url is not None:
        settings.url = url
    if username is not None:
        settings.username = username
    if password is not None:
        settings.password = password
    settings.updated_at = int(time.time())
    await db.commit()
    await db.refresh(settings)
    return settings


def _generate_backup_filename() -> str:
    return f"ccg_gateway_{datetime.now().strftime('%Y%m%d_%H%M%S')}.db"


async def export_to_local() -> Path:
    """Export database to local backup directory, return backup file path."""
    BACKUP_DIR.mkdir(exist_ok=True)
    await engine.dispose()
    backup_filename = _generate_backup_filename()
    backup_path = BACKUP_DIR / backup_filename
    shutil.copy2(DB_FILE, backup_path)
    return backup_path


async def import_from_local(backup_data: bytes) -> None:
    """Import database from uploaded file bytes."""
    await engine.dispose()
    with open(DB_FILE, "wb") as f:
        f.write(backup_data)


async def export_to_webdav(db: AsyncSession) -> str:
    """Export database to WebDAV server, return remote filename."""
    from webdav3.client import Client

    settings = await get_webdav_settings(db)
    if not settings.url:
        raise ValueError("WebDAV URL not configured")

    options = {
        "webdav_hostname": settings.url,
        "webdav_login": settings.username,
        "webdav_password": settings.password,
    }
    client = Client(options)

    if not client.check(WEBDAV_REMOTE_PATH):
        client.mkdir(WEBDAV_REMOTE_PATH)

    await engine.dispose()
    backup_filename = _generate_backup_filename()
    remote_file = f"{WEBDAV_REMOTE_PATH}/{backup_filename}"
    client.upload_sync(remote_path=remote_file, local_path=str(DB_FILE))
    return backup_filename


async def import_from_webdav(db: AsyncSession, filename: str) -> None:
    """Import database from WebDAV server."""
    from webdav3.client import Client

    settings = await get_webdav_settings(db)
    if not settings.url:
        raise ValueError("WebDAV URL not configured")

    options = {
        "webdav_hostname": settings.url,
        "webdav_login": settings.username,
        "webdav_password": settings.password,
    }
    client = Client(options)

    remote_file = f"{WEBDAV_REMOTE_PATH}/{filename}"

    await engine.dispose()
    client.download_sync(remote_path=remote_file, local_path=str(DB_FILE))


async def list_webdav_backups(db: AsyncSession) -> list[dict]:
    """List backup files on WebDAV server."""
    from webdav3.client import Client

    settings = await get_webdav_settings(db)
    if not settings.url:
        raise ValueError("WebDAV URL not configured")

    options = {
        "webdav_hostname": settings.url,
        "webdav_login": settings.username,
        "webdav_password": settings.password,
    }
    client = Client(options)

    if not client.check(WEBDAV_REMOTE_PATH):
        return []

    files = client.list(WEBDAV_REMOTE_PATH)
    backups = []
    for f in files:
        if f.endswith(".db"):
            info = client.info(f"{WEBDAV_REMOTE_PATH}/{f}")
            backups.append({
                "filename": f,
                "size": info.get("size", 0),
                "modified": info.get("modified", ""),
            })
    return sorted(backups, key=lambda x: x["filename"], reverse=True)


def test_webdav_connection(url: str, username: str, password: str) -> bool:
    """Test WebDAV connection."""
    from webdav3.client import Client

    options = {
        "webdav_hostname": url,
        "webdav_login": username,
        "webdav_password": password,
    }
    client = Client(options)
    return client.check("/")
