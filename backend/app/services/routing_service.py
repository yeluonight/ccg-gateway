from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select
from sqlalchemy.orm import selectinload
from typing import Optional
import time
import json
import logging

from app.models.models import Provider
from app.models.log_models import SystemLog

logger = logging.getLogger(__name__)


async def _create_system_log(log_db: AsyncSession, level: str, event_type: str, message: str, provider_name: str = None, details: dict = None):
    """Helper to create system log."""
    log = SystemLog(
        created_at=int(time.time()),
        level=level,
        event_type=event_type,
        provider_name=provider_name,
        message=message,
        details=json.dumps(details, ensure_ascii=False) if details else None
    )
    log_db.add(log)


class RoutingService:
    def __init__(self, db: AsyncSession, log_db: AsyncSession):
        self.db = db
        self.log_db = log_db

    async def select_provider(self, cli_type: str) -> Optional[Provider]:
        """Select provider by availability-first mode (by sort_order, not blacklisted)."""
        now = int(time.time())

        result = await self.db.execute(
            select(Provider)
            .options(selectinload(Provider.model_maps))
            .where(Provider.enabled == 1)
            .where(Provider.cli_type == cli_type)
            .order_by(Provider.sort_order)
        )
        providers = result.scalars().all()

        if not providers:
            logger.warning(f"No enabled providers found for cli_type={cli_type}")
            return None

        skipped_providers = []
        for provider in providers:
            if not self._is_blacklisted(provider, now):
                # Log if we skipped some providers (日志写入失败不影响选路)
                if skipped_providers:
                    try:
                        await _create_system_log(
                            self.log_db, "INFO", "服务商切换",
                            f"切换到服务商 '{provider.name}' (跳过黑名单: {', '.join(skipped_providers)})",
                            provider_name=provider.name,
                            details={"skipped": skipped_providers, "selected": provider.name}
                        )
                        await self.log_db.commit()
                    except Exception:
                        pass
                return provider
            else:
                remaining = provider.blacklisted_until - now
                skipped_providers.append(f"{provider.name}({remaining}s)")

        # All providers are blacklisted
        logger.warning(f"All providers blacklisted for cli_type={cli_type}: {skipped_providers}")
        return None

    def _is_blacklisted(self, provider: Provider, now: int) -> bool:
        """Check if provider is blacklisted."""
        if provider.blacklisted_until is None:
            return False
        return provider.blacklisted_until > now
