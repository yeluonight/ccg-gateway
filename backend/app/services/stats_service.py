from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func, text
from typing import List, Optional
from datetime import date

from app.models.log_models import UsageDaily
from app.schemas.schemas import DailyStatsResponse, ProviderStatsResponse


class StatsService:
    def __init__(self, log_db: AsyncSession):
        self.log_db = log_db

    async def record_request(self, provider_name: str, cli_type: str, success: bool,
                            prompt_tokens: int = 0, completion_tokens: int = 0):
        """Record a request for statistics using atomic upsert."""
        today = date.today().isoformat()
        success_inc = 1 if success else 0
        failure_inc = 0 if success else 1

        # 使用 INSERT ... ON CONFLICT DO UPDATE 原子操作
        await self.log_db.execute(
            text("""
                INSERT INTO usage_daily (usage_date, provider_name, cli_type, request_count, success_count, failure_count, prompt_tokens, completion_tokens)
                VALUES (:usage_date, :provider_name, :cli_type, 1, :success_inc, :failure_inc, :prompt_tokens, :completion_tokens)
                ON CONFLICT (usage_date, provider_name, cli_type) DO UPDATE SET
                    request_count = usage_daily.request_count + 1,
                    success_count = usage_daily.success_count + :success_inc,
                    failure_count = usage_daily.failure_count + :failure_inc,
                    prompt_tokens = usage_daily.prompt_tokens + :prompt_tokens,
                    completion_tokens = usage_daily.completion_tokens + :completion_tokens
            """),
            {
                "usage_date": today,
                "provider_name": provider_name,
                "cli_type": cli_type,
                "success_inc": success_inc,
                "failure_inc": failure_inc,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens
            }
        )
        await self.log_db.commit()

    async def get_daily_stats(
        self,
        start_date: Optional[str] = None,
        end_date: Optional[str] = None,
        cli_type: Optional[str] = None,
        provider_name: Optional[str] = None
    ) -> List[DailyStatsResponse]:
        """Get daily statistics."""
        query = select(UsageDaily)

        if start_date:
            query = query.where(UsageDaily.usage_date >= start_date)
        if end_date:
            query = query.where(UsageDaily.usage_date <= end_date)
        if cli_type:
            query = query.where(UsageDaily.cli_type == cli_type)
        if provider_name:
            query = query.where(UsageDaily.provider_name == provider_name)

        query = query.order_by(UsageDaily.usage_date.desc())

        result = await self.log_db.execute(query)
        rows = result.scalars().all()

        return [
            DailyStatsResponse(
                usage_date=row.usage_date,
                provider_name=row.provider_name,
                cli_type=row.cli_type,
                request_count=row.request_count,
                success_count=row.success_count,
                failure_count=row.failure_count,
                prompt_tokens=row.prompt_tokens,
                completion_tokens=row.completion_tokens
            ) for row in rows
        ]

    async def get_provider_stats(
        self,
        start_date: Optional[str] = None,
        end_date: Optional[str] = None
    ) -> List[ProviderStatsResponse]:
        """Get aggregated statistics by provider and cli_type."""
        query = select(
            UsageDaily.provider_name,
            UsageDaily.cli_type,
            func.sum(UsageDaily.request_count).label("total_requests"),
            func.sum(UsageDaily.success_count).label("total_success"),
            func.sum(UsageDaily.failure_count).label("total_failure"),
            func.sum(UsageDaily.prompt_tokens + UsageDaily.completion_tokens).label("total_tokens")
        )

        if start_date:
            query = query.where(UsageDaily.usage_date >= start_date)
        if end_date:
            query = query.where(UsageDaily.usage_date <= end_date)

        query = query.group_by(UsageDaily.provider_name, UsageDaily.cli_type)

        result = await self.log_db.execute(query)
        rows = result.all()

        return [
            ProviderStatsResponse(
                provider_name=row.provider_name,
                cli_type=row.cli_type,
                total_requests=row.total_requests or 0,
                total_success=row.total_success or 0,
                total_failure=row.total_failure or 0,
                success_rate=(row.total_success / row.total_requests * 100) if row.total_requests else 0,
                total_tokens=row.total_tokens or 0
            ) for row in rows
        ]
