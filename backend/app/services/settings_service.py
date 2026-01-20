from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select
from typing import Optional
import time
import json

# Python 3.11+ has tomllib in stdlib, fallback to tomli
try:
    import tomllib as tomli
    TOMLI_AVAILABLE = True
except ImportError:
    try:
        import tomli
        TOMLI_AVAILABLE = True
    except ImportError:
        tomli = None
        TOMLI_AVAILABLE = False

from app.models.models import GatewaySettings, TimeoutSettings, CliSettings, Provider
from app.schemas.schemas import (
    AllSettingsResponse, GatewaySettingsResponse, TimeoutSettingsResponse,
    CliSettingsResponse, GatewaySettingsUpdate, TimeoutSettingsUpdate, CliSettingsUpdate
)
from app.services.cli_sync_service import sync_cli_settings
from app.services import cli_config_manager
from app.core.config import settings as app_settings


class SettingsService:
    def __init__(self, db: AsyncSession):
        self.db = db

    async def get_all_settings(self) -> AllSettingsResponse:
        """Get all settings."""
        gateway = await self._get_gateway_settings()
        timeouts = await self._get_timeout_settings()
        cli_settings = await self._get_all_cli_settings()

        return AllSettingsResponse(
            gateway=gateway,
            timeouts=timeouts,
            cli_settings=cli_settings
        )

    async def _get_gateway_settings(self) -> GatewaySettingsResponse:
        result = await self.db.execute(select(GatewaySettings).where(GatewaySettings.id == 1))
        settings = result.scalar_one_or_none()
        if not settings:
            return GatewaySettingsResponse(debug_log=False)
        return GatewaySettingsResponse(debug_log=bool(settings.debug_log))

    async def _get_timeout_settings(self) -> TimeoutSettingsResponse:
        result = await self.db.execute(select(TimeoutSettings).where(TimeoutSettings.id == 1))
        settings = result.scalar_one_or_none()
        if not settings:
            return TimeoutSettingsResponse(
                stream_first_byte_timeout=30,
                stream_idle_timeout=60,
                non_stream_timeout=120
            )
        return TimeoutSettingsResponse(
            stream_first_byte_timeout=settings.stream_first_byte_timeout,
            stream_idle_timeout=settings.stream_idle_timeout,
            non_stream_timeout=settings.non_stream_timeout
        )

    async def _get_all_cli_settings(self) -> dict[str, CliSettingsResponse]:
        result = await self.db.execute(select(CliSettings))
        settings = result.scalars().all()

        # 检查实际的 CLI 配置状态
        gateway_host = app_settings.GATEWAY_HOST
        gateway_port = app_settings.GATEWAY_PORT

        cli_status = {}
        for s in settings:
            # 检查实际状态
            actual_enabled = False
            if s.cli_type == "claude_code":
                actual_enabled = cli_config_manager.check_claude_uses_gateway(gateway_host, gateway_port)
            elif s.cli_type == "codex":
                actual_enabled = cli_config_manager.check_codex_uses_gateway(gateway_host, gateway_port)
            elif s.cli_type == "gemini":
                actual_enabled = cli_config_manager.check_gemini_uses_gateway(gateway_host, gateway_port)

            cli_status[s.cli_type] = CliSettingsResponse(
                cli_type=s.cli_type,
                enabled=actual_enabled,
                default_json_config=s.default_json_config
            )

        return cli_status

    async def get_cli_settings(self, cli_type: str) -> Optional[CliSettingsResponse]:
        result = await self.db.execute(
            select(CliSettings).where(CliSettings.cli_type == cli_type)
        )
        settings = result.scalar_one_or_none()
        if not settings:
            return None

        # 检查实际状态
        gateway_host = app_settings.GATEWAY_HOST
        gateway_port = app_settings.GATEWAY_PORT

        actual_enabled = False
        if cli_type == "claude_code":
            actual_enabled = cli_config_manager.check_claude_uses_gateway(gateway_host, gateway_port)
        elif cli_type == "codex":
            actual_enabled = cli_config_manager.check_codex_uses_gateway(gateway_host, gateway_port)
        elif cli_type == "gemini":
            actual_enabled = cli_config_manager.check_gemini_uses_gateway(gateway_host, gateway_port)

        return CliSettingsResponse(
            cli_type=settings.cli_type,
            enabled=actual_enabled,
            default_json_config=settings.default_json_config
        )

    async def update_gateway_settings(self, data: GatewaySettingsUpdate):
        now = int(time.time())
        result = await self.db.execute(select(GatewaySettings).where(GatewaySettings.id == 1))
        settings = result.scalar_one_or_none()

        if not settings:
            settings = GatewaySettings(id=1, updated_at=now)
            self.db.add(settings)

        if data.debug_log is not None:
            settings.debug_log = 1 if data.debug_log else 0
        settings.updated_at = now

        await self.db.commit()

    async def update_timeout_settings(self, data: TimeoutSettingsUpdate):
        now = int(time.time())
        result = await self.db.execute(select(TimeoutSettings).where(TimeoutSettings.id == 1))
        settings = result.scalar_one_or_none()

        if not settings:
            settings = TimeoutSettings(id=1, updated_at=now)
            self.db.add(settings)

        if data.stream_first_byte_timeout is not None:
            settings.stream_first_byte_timeout = data.stream_first_byte_timeout
        if data.stream_idle_timeout is not None:
            settings.stream_idle_timeout = data.stream_idle_timeout
        if data.non_stream_timeout is not None:
            settings.non_stream_timeout = data.non_stream_timeout
        settings.updated_at = now

        await self.db.commit()

    async def update_cli_settings(self, cli_type: str, data: CliSettingsUpdate):
        # 验证配置格式
        if data.default_json_config is not None and data.default_json_config.strip():
            config = data.default_json_config.strip()

            # 对于 claude_code 和 gemini，验证 JSON 格式
            if cli_type in ('claude_code', 'gemini'):
                try:
                    json.loads(config)
                except json.JSONDecodeError as e:
                    raise ValueError(f"JSON 格式错误: {str(e)}")

            # 对于 codex，验证 TOML 格式
            elif cli_type == 'codex':
                if TOMLI_AVAILABLE:
                    try:
                        tomli.loads(config)
                    except Exception as e:
                        raise ValueError(f"TOML 格式错误: {str(e)}")

        # 获取当前设置（只用于获取 default_json_config）
        now = int(time.time())
        result = await self.db.execute(
            select(CliSettings).where(CliSettings.cli_type == cli_type)
        )
        settings = result.scalar_one_or_none()

        if not settings:
            settings = CliSettings(cli_type=cli_type, updated_at=now)
            self.db.add(settings)

        # 检查当前实际状态（基于配置文件）
        gateway_host = app_settings.GATEWAY_HOST
        gateway_port = app_settings.GATEWAY_PORT

        old_enabled = False
        if cli_type == "claude_code":
            old_enabled = cli_config_manager.check_claude_uses_gateway(gateway_host, gateway_port)
        elif cli_type == "codex":
            old_enabled = cli_config_manager.check_codex_uses_gateway(gateway_host, gateway_port)
        elif cli_type == "gemini":
            old_enabled = cli_config_manager.check_gemini_uses_gateway(gateway_host, gateway_port)

        # 如果要启用，检查是否有备份，没有备份则先创建
        if data.enabled and not old_enabled:
            has_backup = cli_config_manager.has_cli_backup(cli_type)
            if not has_backup:
                # 如果不使用当前网关，创建备份
                if not old_enabled:
                    cli_config_manager.backup_cli_config(cli_type)

        # 只更新 default_json_config，不更新 enabled 字段
        if data.default_json_config is not None:
            settings.default_json_config = data.default_json_config
        settings.updated_at = now

        await self.db.commit()

        # 如果 enabled 状态发生变化，同步配置到 CLI
        new_enabled = data.enabled if data.enabled is not None else old_enabled
        if (data.enabled is not None and new_enabled != old_enabled) or (new_enabled and data.default_json_config is not None):
            await self._sync_cli_config(cli_type, settings, new_enabled)

    async def _sync_cli_config(self, cli_type: str, settings: CliSettings, enabled: bool):
        """同步配置到 CLI"""
        base_url = f"http://127.0.0.1:{app_settings.GATEWAY_PORT}"
        api_key = "ccg-gateway"

        sync_cli_settings(cli_type, base_url, api_key, settings.default_json_config, enabled)
