from fastapi import APIRouter, Request, Response
from fastapi.responses import StreamingResponse
import httpx

from app.services.routing_service import RoutingService
from app.services.proxy_service import ProxyService
from app.core.database import async_session_maker, async_log_session_maker

proxy_router = APIRouter()


@proxy_router.api_route("/{path:path}", methods=["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"])
async def proxy_request(request: Request, path: str):
    async with async_session_maker() as db, async_log_session_maker() as log_db:
        routing_service = RoutingService(db, log_db)
        proxy_service = ProxyService(db, log_db, routing_service)
        return await proxy_service.forward_request(request, path)
