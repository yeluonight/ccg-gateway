"""CLI 配置管理服务 - 检查配置状态、备份和恢复"""
import json
import logging
from pathlib import Path
from typing import Optional

# Python 3.11+ has tomllib in stdlib, fallback to tomli
try:
    import tomllib as tomli
except ImportError:
    try:
        import tomli
    except ImportError:
        tomli = None

try:
    import tomli_w
except ImportError:
    tomli_w = None

TOMLI_AVAILABLE = tomli is not None and tomli_w is not None

logger = logging.getLogger(__name__)


def get_home_dir() -> Path:
    """获取用户主目录"""
    return Path.home()


def get_claude_settings_path() -> Path:
    """Claude Code settings 路径: ~/.claude/settings.json"""
    return get_home_dir() / ".claude" / "settings.json"


def get_codex_config_path() -> Path:
    """Codex 配置路径: ~/.codex/config.toml"""
    return get_home_dir() / ".codex" / "config.toml"


def get_codex_auth_path() -> Path:
    """Codex 认证文件路径: ~/.codex/auth.json"""
    return get_home_dir() / ".codex" / "auth.json"


def get_gemini_settings_path() -> Path:
    """Gemini 配置路径: ~/.gemini/settings.json"""
    return get_home_dir() / ".gemini" / "settings.json"


def get_gemini_env_path() -> Path:
    """Gemini .env 路径: ~/.gemini/.env"""
    return get_home_dir() / ".gemini" / ".env"


def get_backup_path(original_path: Path) -> Path:
    """获取备份文件路径"""
    return original_path.parent / f"{original_path.name}.ccg-backup"


def check_claude_uses_gateway(gateway_host: str, gateway_port: int) -> bool:
    """检查 Claude Code 是否使用当前网关"""
    settings_path = get_claude_settings_path()
    if not settings_path.exists():
        return False

    try:
        content = settings_path.read_text(encoding='utf-8').strip()
        if not content:
            return False

        data = json.loads(content)
        base_url = data.get("env", {}).get("ANTHROPIC_BASE_URL", "")

        # 检查是否包含网关地址
        return f"{gateway_host}:{gateway_port}" in base_url or f"localhost:{gateway_port}" in base_url
    except Exception as e:
        logger.error(f"检查 Claude Code 配置失败: {e}")
        return False


def check_codex_uses_gateway(gateway_host: str, gateway_port: int) -> bool:
    """检查 Codex 是否使用当前网关"""
    config_path = get_codex_config_path()
    if not config_path.exists() or not TOMLI_AVAILABLE:
        return False

    try:
        with open(config_path, 'rb') as f:
            data = tomli.load(f)

        # 检查是否使用 ccg-gateway provider
        if data.get("model_provider") != "ccg-gateway":
            return False

        # 检查 provider 的 base_url
        providers = data.get("model_providers", {})
        ccg_provider = providers.get("ccg-gateway", {})
        base_url = ccg_provider.get("base_url", "")

        return f"{gateway_host}:{gateway_port}" in base_url or f"localhost:{gateway_port}" in base_url
    except Exception as e:
        logger.error(f"检查 Codex 配置失败: {e}")
        return False


def check_gemini_uses_gateway(gateway_host: str, gateway_port: int) -> bool:
    """检查 Gemini CLI 是否使用当前网关"""
    env_path = get_gemini_env_path()
    if not env_path.exists():
        return False

    try:
        content = env_path.read_text(encoding='utf-8')

        # 查找 GOOGLE_GEMINI_BASE_URL
        for line in content.split('\n'):
            if line.startswith('GOOGLE_GEMINI_BASE_URL='):
                base_url = line.split('=', 1)[1].strip()
                return f"{gateway_host}:{gateway_port}" in base_url or f"localhost:{gateway_port}" in base_url

        return False
    except Exception as e:
        logger.error(f"检查 Gemini CLI 配置失败: {e}")
        return False


def backup_claude_config() -> bool:
    """备份 Claude Code 配置"""
    settings_path = get_claude_settings_path()
    backup_path = get_backup_path(settings_path)

    if not settings_path.exists():
        logger.debug("Claude Code 配置文件不存在，无需备份")
        return True

    try:
        content = settings_path.read_text(encoding='utf-8')
        backup_path.write_text(content, encoding='utf-8')
        logger.info(f"已备份 Claude Code 配置到 {backup_path}")
        return True
    except Exception as e:
        logger.error(f"备份 Claude Code 配置失败: {e}")
        return False


def backup_codex_config() -> bool:
    """备份 Codex 配置（config.toml 和 auth.json）"""
    config_path = get_codex_config_path()
    auth_path = get_codex_auth_path()

    config_backup = get_backup_path(config_path)
    auth_backup = get_backup_path(auth_path)

    success = True

    # 备份 config.toml
    if config_path.exists():
        try:
            content = config_path.read_bytes()
            config_backup.write_bytes(content)
            logger.info(f"已备份 Codex config.toml 到 {config_backup}")
        except Exception as e:
            logger.error(f"备份 Codex config.toml 失败: {e}")
            success = False

    # 备份 auth.json
    if auth_path.exists():
        try:
            content = auth_path.read_text(encoding='utf-8')
            auth_backup.write_text(content, encoding='utf-8')
            logger.info(f"已备份 Codex auth.json 到 {auth_backup}")
        except Exception as e:
            logger.error(f"备份 Codex auth.json 失败: {e}")
            success = False

    return success


def backup_gemini_config() -> bool:
    """备份 Gemini CLI 配置（settings.json 和 .env）"""
    settings_path = get_gemini_settings_path()
    env_path = get_gemini_env_path()

    settings_backup = get_backup_path(settings_path)
    env_backup = get_backup_path(env_path)

    success = True

    # 备份 settings.json
    if settings_path.exists():
        try:
            content = settings_path.read_text(encoding='utf-8')
            settings_backup.write_text(content, encoding='utf-8')
            logger.info(f"已备份 Gemini settings.json 到 {settings_backup}")
        except Exception as e:
            logger.error(f"备份 Gemini settings.json 失败: {e}")
            success = False

    # 备份 .env
    if env_path.exists():
        try:
            content = env_path.read_text(encoding='utf-8')
            env_backup.write_text(content, encoding='utf-8')
            logger.info(f"已备份 Gemini .env 到 {env_backup}")
        except Exception as e:
            logger.error(f"备份 Gemini .env 失败: {e}")
            success = False

    return success


def restore_claude_config() -> bool:
    """恢复 Claude Code 配置"""
    settings_path = get_claude_settings_path()
    backup_path = get_backup_path(settings_path)

    if not backup_path.exists():
        logger.debug("Claude Code 备份文件不存在，无法恢复")
        return False

    try:
        content = backup_path.read_text(encoding='utf-8')
        settings_path.write_text(content, encoding='utf-8')
        backup_path.unlink()  # 删除备份文件
        logger.info(f"已恢复 Claude Code 配置")
        return True
    except Exception as e:
        logger.error(f"恢复 Claude Code 配置失败: {e}")
        return False


def restore_codex_config() -> bool:
    """恢复 Codex 配置（config.toml 和 auth.json）"""
    config_path = get_codex_config_path()
    auth_path = get_codex_auth_path()

    config_backup = get_backup_path(config_path)
    auth_backup = get_backup_path(auth_path)

    success = True

    # 恢复 config.toml
    if config_backup.exists():
        try:
            content = config_backup.read_bytes()
            config_path.write_bytes(content)
            config_backup.unlink()  # 删除备份文件
            logger.info(f"已恢复 Codex config.toml")
        except Exception as e:
            logger.error(f"恢复 Codex config.toml 失败: {e}")
            success = False

    # 恢复 auth.json
    if auth_backup.exists():
        try:
            content = auth_backup.read_text(encoding='utf-8')
            auth_path.write_text(content, encoding='utf-8')
            auth_backup.unlink()  # 删除备份文件
            logger.info(f"已恢复 Codex auth.json")
        except Exception as e:
            logger.error(f"恢复 Codex auth.json 失败: {e}")
            success = False

    if not config_backup.exists() and not auth_backup.exists():
        logger.debug("Codex 备份文件不存在，无法恢复")
        return False

    return success


def restore_gemini_config() -> bool:
    """恢复 Gemini CLI 配置（settings.json 和 .env）"""
    settings_path = get_gemini_settings_path()
    env_path = get_gemini_env_path()

    settings_backup = get_backup_path(settings_path)
    env_backup = get_backup_path(env_path)

    success = True

    # 恢复 settings.json
    if settings_backup.exists():
        try:
            content = settings_backup.read_text(encoding='utf-8')
            settings_path.write_text(content, encoding='utf-8')
            settings_backup.unlink()  # 删除备份文件
            logger.info(f"已恢复 Gemini settings.json")
        except Exception as e:
            logger.error(f"恢复 Gemini settings.json 失败: {e}")
            success = False

    # 恢复 .env
    if env_backup.exists():
        try:
            content = env_backup.read_text(encoding='utf-8')
            env_path.write_text(content, encoding='utf-8')
            env_backup.unlink()  # 删除备份文件
            logger.info(f"已恢复 Gemini .env")
        except Exception as e:
            logger.error(f"恢复 Gemini .env 失败: {e}")
            success = False

    return success


def has_claude_backup() -> bool:
    """检查是否存在 Claude Code 备份"""
    return get_backup_path(get_claude_settings_path()).exists()


def has_codex_backup() -> bool:
    """检查是否存在 Codex 备份"""
    config_backup = get_backup_path(get_codex_config_path())
    auth_backup = get_backup_path(get_codex_auth_path())
    return config_backup.exists() or auth_backup.exists()


def has_gemini_backup() -> bool:
    """检查是否存在 Gemini CLI 备份"""
    settings_backup = get_backup_path(get_gemini_settings_path())
    env_backup = get_backup_path(get_gemini_env_path())
    return settings_backup.exists() or env_backup.exists()


def clear_claude_config() -> bool:
    """清空 Claude Code 配置"""
    settings_path = get_claude_settings_path()

    if not settings_path.exists():
        return True

    try:
        settings_path.write_text('{}', encoding='utf-8')
        logger.info("已清空 Claude Code 配置")
        return True
    except Exception as e:
        logger.error(f"清空 Claude Code 配置失败: {e}")
        return False


def clear_codex_config() -> bool:
    """清空 Codex 配置（config.toml 和 auth.json）"""
    config_path = get_codex_config_path()
    auth_path = get_codex_auth_path()

    success = True

    # 清空 config.toml
    if config_path.exists() and TOMLI_AVAILABLE:
        try:
            with open(config_path, 'wb') as f:
                tomli_w.dump({}, f)
            logger.info("已清空 Codex config.toml")
        except Exception as e:
            logger.error(f"清空 Codex config.toml 失败: {e}")
            success = False

    # 清空 auth.json
    if auth_path.exists():
        try:
            auth_path.write_text('{}', encoding='utf-8')
            logger.info("已清空 Codex auth.json")
        except Exception as e:
            logger.error(f"清空 Codex auth.json 失败: {e}")
            success = False

    return success


def clear_gemini_config() -> bool:
    """清空 Gemini CLI 配置"""
    settings_path = get_gemini_settings_path()
    env_path = get_gemini_env_path()

    success = True

    # 清空 settings.json
    if settings_path.exists():
        try:
            settings_path.write_text('{}', encoding='utf-8')
            logger.info("已清空 Gemini settings.json")
        except Exception as e:
            logger.error(f"清空 Gemini settings.json 失败: {e}")
            success = False

    # 清空 .env
    if env_path.exists():
        try:
            env_path.write_text('', encoding='utf-8')
            logger.info("已清空 Gemini .env")
        except Exception as e:
            logger.error(f"清空 Gemini .env 失败: {e}")
            success = False

    return success


def check_cli_status(gateway_host: str, gateway_port: int) -> dict:
    """检查所有 CLI 的状态"""
    return {
        "claude_code": check_claude_uses_gateway(gateway_host, gateway_port),
        "codex": check_codex_uses_gateway(gateway_host, gateway_port),
        "gemini": check_gemini_uses_gateway(gateway_host, gateway_port)
    }


def backup_cli_config(cli_type: str) -> bool:
    """备份指定 CLI 的配置"""
    if cli_type == "claude_code":
        return backup_claude_config()
    elif cli_type == "codex":
        return backup_codex_config()
    elif cli_type == "gemini":
        return backup_gemini_config()
    else:
        logger.warning(f"未知的 CLI 类型: {cli_type}")
        return False


def restore_cli_config(cli_type: str) -> bool:
    """恢复指定 CLI 的配置"""
    if cli_type == "claude_code":
        return restore_claude_config()
    elif cli_type == "codex":
        return restore_codex_config()
    elif cli_type == "gemini":
        return restore_gemini_config()
    else:
        logger.warning(f"未知的 CLI 类型: {cli_type}")
        return False


def has_cli_backup(cli_type: str) -> bool:
    """检查指定 CLI 是否有备份"""
    if cli_type == "claude_code":
        return has_claude_backup()
    elif cli_type == "codex":
        return has_codex_backup()
    elif cli_type == "gemini":
        return has_gemini_backup()
    else:
        return False


def clear_cli_config(cli_type: str) -> bool:
    """清空指定 CLI 的配置"""
    if cli_type == "claude_code":
        return clear_claude_config()
    elif cli_type == "codex":
        return clear_codex_config()
    elif cli_type == "gemini":
        return clear_gemini_config()
    else:
        logger.warning(f"未知的 CLI 类型: {cli_type}")
        return False
