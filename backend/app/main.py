import asyncio
import logging
import os
import sys
from contextlib import asynccontextmanager
from pathlib import Path
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from fastapi.responses import FileResponse

logging.basicConfig(level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s")

from app.core.config import settings
from app.core.database import init_db, close_db
from app.core.uptime import init_start_time
from app.api.admin import admin_router
from app.api.proxy import proxy_router
from app.services.init_service import init_default_data


def get_frontend_dist() -> Path | None:
    """Get frontend dist path for desktop mode."""
    if not os.getenv("DESKTOP_MODE"):
        return None
    # PyInstaller bundled
    if hasattr(sys, '_MEIPASS'):
        dist = Path(sys._MEIPASS) / "frontend" / "dist"
        if dist.exists():
            return dist
    # Development
    dist = Path(__file__).parent.parent.parent / "frontend" / "dist"
    if dist.exists():
        return dist
    return None


frontend_dist = get_frontend_dist()


@asynccontextmanager
async def lifespan(app: FastAPI):
    init_start_time()
    await init_db()
    await init_default_data()
    try:
        yield
    except asyncio.CancelledError:
        pass
    finally:
        await close_db()


app = FastAPI(
    title=settings.PROJECT_NAME,
    version=settings.VERSION,
    lifespan=lifespan
)

# CORS
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Routes
app.include_router(admin_router, prefix="/admin/v1")


@app.get("/health")
async def health_check():
    return {"status": "ok"}


# Desktop mode: serve frontend static files (must be before proxy_router)
if frontend_dist:
    app.mount("/assets", StaticFiles(directory=frontend_dist / "assets"), name="assets")

    @app.get("/")
    async def serve_index():
        return FileResponse(frontend_dist / "index.html")

    @app.get("/{full_path:path}")
    async def serve_spa(full_path: str):
        file_path = frontend_dist / full_path
        if file_path.is_file():
            return FileResponse(file_path)
        return FileResponse(frontend_dist / "index.html")
else:
    # Only include proxy router when not in desktop mode (or no frontend dist)
    app.include_router(proxy_router)
