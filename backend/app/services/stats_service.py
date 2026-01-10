from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func
from typing import List, Optional
from datetime import date, datetime, timedelta

from app.models.models import UsageDaily, Provider
from app.schemas.schemas import DailyStatsResponse, ProviderStatsResponse


class StatsService:
    def __init__(self, db: AsyncSession):
        self.db = db

    async def record_request(self, provider_id: int, cli_type: str, success: bool,
                            prompt_tokens: int = 0, completion_tokens: int = 0):
        """Record a request for statistics."""
        today = date.today().isoformat()

        result = await self.db.execute(
            select(UsageDaily).where(
                UsageDaily.usage_date == today,
                UsageDaily.provider_id == provider_id,
                UsageDaily.cli_type == cli_type
            )
        )
        usage = result.scalar_one_or_none()

        if not usage:
            usage = UsageDaily(
                usage_date=today,
                provider_id=provider_id,
                cli_type=cli_type,
                request_count=0,
                success_count=0,
                failure_count=0,
                prompt_tokens=0,
                completion_tokens=0
            )
            self.db.add(usage)

        usage.request_count += 1
        if success:
            usage.success_count += 1
        else:
            usage.failure_count += 1
        usage.prompt_tokens += prompt_tokens
        usage.completion_tokens += completion_tokens

        await self.db.commit()

    async def get_daily_stats(
        self,
        start_date: Optional[str] = None,
        end_date: Optional[str] = None,
        cli_type: Optional[str] = None,
        provider_id: Optional[int] = None
    ) -> List[DailyStatsResponse]:
        """Get daily statistics."""
        query = select(UsageDaily, Provider.name).join(
            Provider, UsageDaily.provider_id == Provider.id
        )

        if start_date:
            query = query.where(UsageDaily.usage_date >= start_date)
        if end_date:
            query = query.where(UsageDaily.usage_date <= end_date)
        if cli_type:
            query = query.where(UsageDaily.cli_type == cli_type)
        if provider_id:
            query = query.where(UsageDaily.provider_id == provider_id)

        query = query.order_by(UsageDaily.usage_date.desc())

        result = await self.db.execute(query)
        rows = result.all()

        return [
            DailyStatsResponse(
                usage_date=row.UsageDaily.usage_date,
                provider_id=row.UsageDaily.provider_id,
                provider_name=row.name,
                cli_type=row.UsageDaily.cli_type,
                request_count=row.UsageDaily.request_count,
                success_count=row.UsageDaily.success_count,
                failure_count=row.UsageDaily.failure_count,
                prompt_tokens=row.UsageDaily.prompt_tokens,
                completion_tokens=row.UsageDaily.completion_tokens
            ) for row in rows
        ]

    async def get_provider_stats(
        self,
        start_date: Optional[str] = None,
        end_date: Optional[str] = None
    ) -> List[ProviderStatsResponse]:
        """Get aggregated statistics by provider and cli_type."""
        query = select(
            UsageDaily.provider_id,
            Provider.name,
            UsageDaily.cli_type,
            func.sum(UsageDaily.request_count).label("total_requests"),
            func.sum(UsageDaily.success_count).label("total_success"),
            func.sum(UsageDaily.failure_count).label("total_failure"),
            func.sum(UsageDaily.prompt_tokens + UsageDaily.completion_tokens).label("total_tokens")
        ).join(
            Provider, UsageDaily.provider_id == Provider.id
        )

        if start_date:
            query = query.where(UsageDaily.usage_date >= start_date)
        if end_date:
            query = query.where(UsageDaily.usage_date <= end_date)

        query = query.group_by(UsageDaily.provider_id, Provider.name, UsageDaily.cli_type)

        result = await self.db.execute(query)
        rows = result.all()

        return [
            ProviderStatsResponse(
                provider_id=row.provider_id,
                provider_name=row.name,
                cli_type=row.cli_type,
                total_requests=row.total_requests or 0,
                total_success=row.total_success or 0,
                total_failure=row.total_failure or 0,
                success_rate=(row.total_success / row.total_requests * 100) if row.total_requests else 0,
                total_tokens=row.total_tokens or 0
            ) for row in rows
        ]
