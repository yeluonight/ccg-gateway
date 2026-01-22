from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, update, delete, text
from sqlalchemy.orm import selectinload
from typing import List, Optional, Dict
import time
import asyncio

from app.models.models import Provider, ProviderModelMap
from app.schemas.schemas import ProviderCreate, ProviderUpdate, ProviderResponse, ModelMapResponse

# Per-provider locks for failure recording
_provider_locks: Dict[int, asyncio.Lock] = {}
_locks_lock = asyncio.Lock()


async def _get_provider_lock(provider_id: int) -> asyncio.Lock:
    async with _locks_lock:
        if provider_id not in _provider_locks:
            _provider_locks[provider_id] = asyncio.Lock()
        return _provider_locks[provider_id]


async def _create_system_log(log_db: AsyncSession, level: str, event_type: str, message: str, provider_name: str = None, details: dict = None):
    """Helper to create system log without circular import."""
    import json
    from app.models.log_models import SystemLog
    log = SystemLog(
        created_at=int(time.time()),
        level=level,
        event_type=event_type,
        provider_name=provider_name,
        message=message,
        details=json.dumps(details, ensure_ascii=False) if details else None
    )
    log_db.add(log)


class ProviderService:
    def __init__(self, db: AsyncSession, log_db: AsyncSession = None):
        self.db = db
        self.log_db = log_db

    async def list_providers(self, cli_type: Optional[str] = None) -> List[ProviderResponse]:
        now = int(time.time())
        query = select(Provider).options(selectinload(Provider.model_maps))
        if cli_type:
            query = query.where(Provider.cli_type == cli_type)
        query = query.order_by(Provider.sort_order, Provider.id)
        result = await self.db.execute(query)
        providers = result.scalars().all()

        responses = []
        for p in providers:
            model_maps = [
                ModelMapResponse(
                    id=m.id,
                    source_model=m.source_model,
                    target_model=m.target_model,
                    enabled=bool(m.enabled)
                ) for m in p.model_maps
            ]
            is_blacklisted = p.blacklisted_until is not None and p.blacklisted_until > now
            responses.append(ProviderResponse(
                id=p.id,
                cli_type=p.cli_type,
                name=p.name,
                base_url=p.base_url,
                api_key=p.api_key,
                enabled=bool(p.enabled),
                failure_threshold=p.failure_threshold,
                blacklist_minutes=p.blacklist_minutes,
                consecutive_failures=p.consecutive_failures,
                blacklisted_until=p.blacklisted_until,
                sort_order=p.sort_order,
                model_maps=model_maps,
                is_blacklisted=is_blacklisted
            ))
        return responses

    async def get_provider(self, provider_id: int) -> Optional[ProviderResponse]:
        now = int(time.time())
        result = await self.db.execute(
            select(Provider).options(selectinload(Provider.model_maps)).where(Provider.id == provider_id)
        )
        p = result.scalar_one_or_none()
        if not p:
            return None

        model_maps = [
            ModelMapResponse(
                id=m.id,
                source_model=m.source_model,
                target_model=m.target_model,
                enabled=bool(m.enabled)
            ) for m in p.model_maps
        ]
        is_blacklisted = p.blacklisted_until is not None and p.blacklisted_until > now
        return ProviderResponse(
            id=p.id,
            cli_type=p.cli_type,
            name=p.name,
            base_url=p.base_url,
            api_key=p.api_key,
            enabled=bool(p.enabled),
            failure_threshold=p.failure_threshold,
            blacklist_minutes=p.blacklist_minutes,
            consecutive_failures=p.consecutive_failures,
            blacklisted_until=p.blacklisted_until,
            sort_order=p.sort_order,
            model_maps=model_maps,
            is_blacklisted=is_blacklisted
        )

    async def create_provider(self, data: ProviderCreate) -> ProviderResponse:
        now = int(time.time())

        # Get max sort_order
        result = await self.db.execute(select(Provider.sort_order).order_by(Provider.sort_order.desc()).limit(1))
        max_order = result.scalar() or 0

        # Insert using raw SQL to get lastrowid reliably
        result = await self.db.execute(
            text("""
                INSERT INTO providers (cli_type, name, base_url, api_key, enabled, failure_threshold, blacklist_minutes, consecutive_failures, sort_order, created_at, updated_at)
                VALUES (:cli_type, :name, :base_url, :api_key, :enabled, :failure_threshold, :blacklist_minutes, :consecutive_failures, :sort_order, :created_at, :updated_at)
            """),
            {
                'cli_type': data.cli_type.value,
                'name': data.name,
                'base_url': data.base_url,
                'api_key': data.api_key,
                'enabled': 1 if data.enabled else 0,
                'failure_threshold': data.failure_threshold,
                'blacklist_minutes': data.blacklist_minutes,
                'consecutive_failures': 0,
                'sort_order': max_order + 1,
                'created_at': now,
                'updated_at': now
            }
        )
        provider_id = result.lastrowid

        # Add model maps
        for mm in data.model_maps:
            model_map = ProviderModelMap(
                provider_id=provider_id,
                source_model=mm.source_model,
                target_model=mm.target_model,
                enabled=1 if mm.enabled else 0
            )
            self.db.add(model_map)

        await self.db.commit()
        return await self.get_provider(provider_id)

    async def update_provider(self, provider_id: int, data: ProviderUpdate) -> Optional[ProviderResponse]:
        result = await self.db.execute(
            select(Provider).where(Provider.id == provider_id)
        )
        provider = result.scalar_one_or_none()
        if not provider:
            return None

        now = int(time.time())
        update_data = data.model_dump(exclude_unset=True, exclude={"model_maps"})
        if "enabled" in update_data:
            update_data["enabled"] = 1 if update_data["enabled"] else 0
        update_data["updated_at"] = now

        for key, value in update_data.items():
            setattr(provider, key, value)

        # Update model maps if provided
        if data.model_maps is not None:
            await self.db.execute(
                delete(ProviderModelMap).where(ProviderModelMap.provider_id == provider_id)
            )
            for mm in data.model_maps:
                model_map = ProviderModelMap(
                    provider_id=provider_id,
                    source_model=mm.source_model,
                    target_model=mm.target_model,
                    enabled=1 if mm.enabled else 0
                )
                self.db.add(model_map)

        await self.db.commit()
        return await self.get_provider(provider_id)

    async def delete_provider(self, provider_id: int) -> bool:
        result = await self.db.execute(
            select(Provider).where(Provider.id == provider_id)
        )
        provider = result.scalar_one_or_none()
        if not provider:
            return False

        await self.db.delete(provider)
        await self.db.commit()
        return True

    async def reorder_providers(self, ids: List[int]):
        for idx, provider_id in enumerate(ids):
            await self.db.execute(
                update(Provider).where(Provider.id == provider_id).values(sort_order=idx)
            )
        await self.db.commit()

    async def reset_failures(self, provider_id: int) -> bool:
        result = await self.db.execute(
            select(Provider).where(Provider.id == provider_id)
        )
        provider = result.scalar_one_or_none()
        if not provider:
            return False

        provider.consecutive_failures = 0
        await self.db.commit()
        return True

    async def unblacklist(self, provider_id: int) -> bool:
        result = await self.db.execute(
            select(Provider).where(Provider.id == provider_id)
        )
        provider = result.scalar_one_or_none()
        if not provider:
            return False

        provider.blacklisted_until = None
        provider.consecutive_failures = 0
        await self.db.commit()
        return True

    async def record_success(self, provider_id: int):
        result = await self.db.execute(
            select(Provider).where(Provider.id == provider_id)
        )
        provider = result.scalar_one_or_none()
        if provider and provider.consecutive_failures > 0:
            previous_failures = provider.consecutive_failures
            provider.consecutive_failures = 0
            await self.db.commit()
            # 日志写入失败不影响主流程
            if self.log_db:
                try:
                    await _create_system_log(
                        self.log_db, "INFO", "失败重置",
                        f"服务商 '{provider.name}' 失败计数已重置 (原为 {previous_failures}) - 请求成功",
                        provider_name=provider.name,
                        details={"previous_failures": previous_failures}
                    )
                    await self.log_db.commit()
                except Exception:
                    pass

    async def record_failure(self, provider_id: int):
        lock = await _get_provider_lock(provider_id)
        async with lock:
            now = int(time.time())

            result = await self.db.execute(
                text("SELECT name, consecutive_failures, failure_threshold, blacklist_minutes, blacklisted_until FROM providers WHERE id = :id"),
                {"id": provider_id}
            )
            row = result.fetchone()
            if not row:
                return

            name, failures, threshold, blacklist_minutes, blacklisted_until = row

            # Skip if already blacklisted (concurrent requests that started before blacklist)
            if blacklisted_until and blacklisted_until > now:
                return

            new_failures = failures + 1

            if new_failures >= threshold:
                blacklist_until = now + blacklist_minutes * 60
                await self.db.execute(
                    text("UPDATE providers SET consecutive_failures = 0, blacklisted_until = :until WHERE id = :id"),
                    {"until": blacklist_until, "id": provider_id}
                )
            else:
                await self.db.execute(
                    text("UPDATE providers SET consecutive_failures = :failures WHERE id = :id"),
                    {"failures": new_failures, "id": provider_id}
                )

            await self.db.commit()

            # 日志写入失败不影响主流程
            if self.log_db:
                try:
                    await _create_system_log(
                        self.log_db, "WARN", "服务商失败",
                        f"服务商 '{name}' 失败，连续失败次数: {new_failures}/{threshold}",
                        provider_name=name,
                        details={"consecutive_failures": new_failures, "threshold": threshold}
                    )
                    if new_failures >= threshold:
                        await _create_system_log(
                            self.log_db, "ERROR", "服务商黑名单",
                            f"服务商 '{name}' 已加入黑名单 {blacklist_minutes} 分钟 (达到阈值 {threshold})",
                            provider_name=name,
                            details={"blacklist_minutes": blacklist_minutes, "blacklisted_until": blacklist_until}
                        )
                    await self.log_db.commit()
                except Exception:
                    pass
