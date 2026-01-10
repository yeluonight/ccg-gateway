from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select
from typing import Optional
import time
import json

from app.models.models import Provider, SystemLog


async def _create_system_log(db: AsyncSession, level: str, event_type: str, message: str, provider_name: str = None, details: dict = None):
    """Helper to create system log."""
    log = SystemLog(
        created_at=int(time.time()),
        level=level,
        event_type=event_type,
        provider_name=provider_name,
        message=message,
        details=json.dumps(details, ensure_ascii=False) if details else None
    )
    db.add(log)


class RoutingService:
    def __init__(self, db: AsyncSession):
        self.db = db

    async def select_provider(self, cli_type: str) -> Optional[Provider]:
        """Select provider by availability-first mode (by sort_order, not blacklisted)."""
        now = int(time.time())

        result = await self.db.execute(
            select(Provider)
            .where(Provider.enabled == 1)
            .where(Provider.cli_type == cli_type)
            .order_by(Provider.sort_order)
        )
        providers = result.scalars().all()

        skipped_providers = []
        for provider in providers:
            if not self._is_blacklisted(provider, now):
                # Log if we skipped some providers
                if skipped_providers:
                    await _create_system_log(
                        self.db, "INFO", "provider_switch",
                        f"Switched to provider '{provider.name}' (skipped blacklisted: {', '.join(skipped_providers)})",
                        provider_name=provider.name,
                        details={"skipped": skipped_providers, "selected": provider.name}
                    )
                    await self.db.commit()
                return provider
            else:
                skipped_providers.append(provider.name)

        return None

    def _is_blacklisted(self, provider: Provider, now: int) -> bool:
        """Check if provider is blacklisted."""
        if provider.blacklisted_until is None:
            return False
        return provider.blacklisted_until > now
