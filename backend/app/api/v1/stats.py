from fastapi import APIRouter, Depends, Query
from sqlalchemy.ext.asyncio import AsyncSession
from typing import List, Optional

from app.core.database import get_log_db
from app.schemas.schemas import DailyStatsResponse, ProviderStatsResponse
from app.services.stats_service import StatsService

router = APIRouter()


@router.get("/daily", response_model=List[DailyStatsResponse])
async def get_daily_stats(
    start_date: Optional[str] = Query(None, description="Start date (YYYY-MM-DD)"),
    end_date: Optional[str] = Query(None, description="End date (YYYY-MM-DD)"),
    cli_type: Optional[str] = Query(None),
    provider_name: Optional[str] = Query(None),
    log_db: AsyncSession = Depends(get_log_db)
):
    service = StatsService(log_db)
    return await service.get_daily_stats(start_date, end_date, cli_type, provider_name)


@router.get("/providers", response_model=List[ProviderStatsResponse])
async def get_provider_stats(
    start_date: Optional[str] = Query(None, description="Start date (YYYY-MM-DD)"),
    end_date: Optional[str] = Query(None, description="End date (YYYY-MM-DD)"),
    log_db: AsyncSession = Depends(get_log_db)
):
    service = StatsService(log_db)
    return await service.get_provider_stats(start_date, end_date)
