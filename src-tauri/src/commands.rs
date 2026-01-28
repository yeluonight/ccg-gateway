use crate::config::get_data_dir;
use crate::db::models::{
    Provider, ProviderCreate, ProviderResponse, ProviderUpdate,
    GatewaySettings, TimeoutSettings, TimeoutSettingsUpdate,
    CliSettingsRow, CliSettingsResponse, CliSettingsUpdate,
    RequestLogItem, RequestLogDetail, PaginatedLogs,
    SystemLogItem, SystemLogListResponse,
    DailyStats, ProviderStatsRow, ProviderStatsResponse,
    McpConfig, McpCliFlag, McpResponse, McpCreate, McpUpdate,
    PromptPreset, PromptCliFlag, PromptResponse, PromptCreate, PromptUpdate,
    WebdavSettings, WebdavSettingsUpdate, WebdavBackup,
    ProjectInfo, SessionInfo, PaginatedProjects, PaginatedSessions, SessionMessage,
    SystemStatus,
};
use crate::LogDb;
use sqlx::SqlitePool;
use tauri::State;

type Result<T> = std::result::Result<T, String>;

#[tauri::command]
pub async fn get_providers(
    db: State<'_, SqlitePool>,
    cli_type: Option<String>,
) -> Result<Vec<ProviderResponse>> {
    let providers = if let Some(ct) = cli_type {
        sqlx::query_as::<_, Provider>(
            "SELECT * FROM providers WHERE cli_type = ? ORDER BY sort_order, id",
        )
        .bind(&ct)
        .fetch_all(db.inner())
        .await
    } else {
        sqlx::query_as::<_, Provider>("SELECT * FROM providers ORDER BY sort_order, id")
            .fetch_all(db.inner())
            .await
    };

    let providers = providers.map_err(|e| e.to_string())?;
    let mut results = Vec::new();

    for provider in providers {
        let mut response = ProviderResponse::from(provider.clone());

        // Load model maps
        let maps: Vec<(i64, String, String, i64)> = sqlx::query_as(
            "SELECT id, source_model, target_model, enabled FROM provider_model_map WHERE provider_id = ? ORDER BY id",
        )
        .bind(provider.id)
        .fetch_all(db.inner())
        .await
        .map_err(|e| e.to_string())?;

        response.model_maps = maps
            .into_iter()
            .map(|(id, source_model, target_model, enabled)| crate::db::models::ModelMapResponse {
                id,
                source_model,
                target_model,
                enabled: enabled != 0,
            })
            .collect();

        results.push(response);
    }

    Ok(results)
}

#[tauri::command]
pub async fn get_provider(db: State<'_, SqlitePool>, id: i64) -> Result<ProviderResponse> {
    let provider = sqlx::query_as::<_, Provider>("SELECT * FROM providers WHERE id = ?")
        .bind(id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Provider not found".to_string())?;

    let mut response = ProviderResponse::from(provider);

    // Load model maps
    let maps: Vec<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT id, source_model, target_model, enabled FROM provider_model_map WHERE provider_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    response.model_maps = maps
        .into_iter()
        .map(|(id, source_model, target_model, enabled)| crate::db::models::ModelMapResponse {
            id,
            source_model,
            target_model,
            enabled: enabled != 0,
        })
        .collect();

    Ok(response)
}

#[tauri::command]
pub async fn create_provider(
    db: State<'_, SqlitePool>,
    log_db: State<'_, LogDb>,
    input: ProviderCreate,
) -> Result<ProviderResponse> {
    let now = chrono::Utc::now().timestamp();
    let cli_type = input.cli_type.unwrap_or_else(|| "claude_code".to_string());
    let provider_name = input.name.clone();

    let result = sqlx::query(
        r#"
        INSERT INTO providers (cli_type, name, base_url, api_key, enabled, failure_threshold, blacklist_minutes, consecutive_failures, sort_order, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, 0, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM providers), ?, ?)
        "#,
    )
    .bind(&cli_type)
    .bind(&input.name)
    .bind(&input.base_url)
    .bind(&input.api_key)
    .bind(input.enabled.unwrap_or(true) as i64)
    .bind(input.failure_threshold.unwrap_or(3))
    .bind(input.blacklist_minutes.unwrap_or(10))
    .bind(now)
    .bind(now)
    .execute(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    let id = result.last_insert_rowid();

    // Insert model maps if provided
    if let Some(model_maps) = input.model_maps {
        for map in model_maps {
            sqlx::query(
                "INSERT INTO provider_model_map (provider_id, source_model, target_model, enabled) VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(&map.source_model)
            .bind(&map.target_model)
            .bind(map.enabled as i64)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    // Log system event
    let _ = crate::services::stats::record_system_log(
        &log_db.0,
        "info",
        "provider_created",
        &format!("Provider {} created", provider_name),
        Some(&provider_name),
        None,
    ).await;

    get_provider(db, id).await
}

#[tauri::command]
pub async fn update_provider(
    db: State<'_, SqlitePool>,
    log_db: State<'_, LogDb>,
    id: i64,
    input: ProviderUpdate,
) -> Result<ProviderResponse> {
    let now = chrono::Utc::now().timestamp();

    // Get provider name for logging
    let provider_name: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM providers WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    let provider_name = provider_name.map(|(n,)| n).unwrap_or_else(|| format!("Provider#{}", id));

    // Check if model maps will be updated (before moving)
    let has_model_maps_update = input.model_maps.is_some();

    // Build dynamic update query
    let mut updates = vec!["updated_at = ?".to_string()];
    let mut has_updates = false;

    if input.name.is_some() {
        updates.push("name = ?".to_string());
        has_updates = true;
    }
    if input.base_url.is_some() {
        updates.push("base_url = ?".to_string());
        has_updates = true;
    }
    if input.api_key.is_some() {
        updates.push("api_key = ?".to_string());
        has_updates = true;
    }
    if input.enabled.is_some() {
        updates.push("enabled = ?".to_string());
        has_updates = true;
    }
    if input.failure_threshold.is_some() {
        updates.push("failure_threshold = ?".to_string());
        has_updates = true;
    }
    if input.blacklist_minutes.is_some() {
        updates.push("blacklist_minutes = ?".to_string());
        has_updates = true;
    }

    if has_updates {
        let query = format!("UPDATE providers SET {} WHERE id = ?", updates.join(", "));
        let mut q = sqlx::query(&query).bind(now);

        if let Some(ref name) = input.name {
            q = q.bind(name);
        }
        if let Some(ref base_url) = input.base_url {
            q = q.bind(base_url);
        }
        if let Some(ref api_key) = input.api_key {
            q = q.bind(api_key);
        }
        if let Some(enabled) = input.enabled {
            q = q.bind(enabled as i64);
        }
        if let Some(failure_threshold) = input.failure_threshold {
            q = q.bind(failure_threshold);
        }
        if let Some(blacklist_minutes) = input.blacklist_minutes {
            q = q.bind(blacklist_minutes);
        }

        q.bind(id)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
    }

    // Update model maps if provided
    if let Some(model_maps) = input.model_maps {
        // Delete existing maps
        sqlx::query("DELETE FROM provider_model_map WHERE provider_id = ?")
            .bind(id)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;

        // Insert new maps
        for map in model_maps {
            sqlx::query(
                "INSERT INTO provider_model_map (provider_id, source_model, target_model, enabled) VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(&map.source_model)
            .bind(&map.target_model)
            .bind(map.enabled as i64)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    // Log system event (only if there were actual updates)
    if has_updates || has_model_maps_update {
        let _ = crate::services::stats::record_system_log(
            &log_db.0,
            "info",
            "provider_updated",
            &format!("Provider {} updated", provider_name),
            Some(&provider_name),
            None,
        ).await;
    }

    get_provider(db, id).await
}

#[tauri::command]
pub async fn delete_provider(
    db: State<'_, SqlitePool>,
    log_db: State<'_, LogDb>,
    id: i64,
) -> Result<()> {
    // Get provider name before deletion
    let provider_name: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM providers WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    let provider_name = provider_name.map(|(n,)| n).unwrap_or_else(|| format!("Provider#{}", id));

    // Delete associated model maps first (cascade delete)
    sqlx::query("DELETE FROM provider_model_map WHERE provider_id = ?")
        .bind(id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    // Then delete the provider
    sqlx::query("DELETE FROM providers WHERE id = ?")
        .bind(id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    // Log system event
    let _ = crate::services::stats::record_system_log(
        &log_db.0,
        "info",
        "provider_deleted",
        &format!("Provider {} deleted", provider_name),
        Some(&provider_name),
        None,
    ).await;

    Ok(())
}

#[tauri::command]
pub async fn reorder_providers(db: State<'_, SqlitePool>, ids: Vec<i64>) -> Result<()> {
    for (idx, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE providers SET sort_order = ? WHERE id = ?")
            .bind(idx as i64)
            .bind(id)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn reset_provider_failures(
    db: State<'_, SqlitePool>,
    log_db: State<'_, LogDb>,
    id: i64,
) -> Result<()> {
    // Get provider name for logging
    let provider_name: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM providers WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    let provider_name = provider_name.map(|(n,)| n).unwrap_or_else(|| format!("Provider#{}", id));

    sqlx::query("UPDATE providers SET consecutive_failures = 0, blacklisted_until = NULL WHERE id = ?")
        .bind(id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    // Log system event
    let _ = crate::services::stats::record_system_log(
        &log_db.0,
        "info",
        "provider_reset",
        &format!("Provider {} status manually reset", provider_name),
        Some(&provider_name),
        None,
    ).await;

    Ok(())
}

// Settings commands
#[tauri::command]
pub async fn get_gateway_settings(db: State<'_, SqlitePool>) -> Result<GatewaySettings> {
    sqlx::query_as::<_, GatewaySettings>("SELECT debug_log FROM gateway_settings WHERE id = 1")
        .fetch_one(db.inner())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_gateway_settings(db: State<'_, SqlitePool>, debug_log: bool) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE gateway_settings SET debug_log = ?, updated_at = ? WHERE id = 1")
        .bind(debug_log as i64)
        .bind(now)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_timeout_settings(db: State<'_, SqlitePool>) -> Result<TimeoutSettings> {
    sqlx::query_as::<_, TimeoutSettings>(
        "SELECT stream_first_byte_timeout, stream_idle_timeout, non_stream_timeout FROM timeout_settings WHERE id = 1",
    )
    .fetch_one(db.inner())
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_timeout_settings(
    db: State<'_, SqlitePool>,
    input: TimeoutSettingsUpdate,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let current = get_timeout_settings(db.clone()).await?;

    sqlx::query(
        "UPDATE timeout_settings SET stream_first_byte_timeout = ?, stream_idle_timeout = ?, non_stream_timeout = ?, updated_at = ? WHERE id = 1",
    )
    .bind(input.stream_first_byte_timeout.unwrap_or(current.stream_first_byte_timeout))
    .bind(input.stream_idle_timeout.unwrap_or(current.stream_idle_timeout))
    .bind(input.non_stream_timeout.unwrap_or(current.non_stream_timeout))
    .bind(now)
    .execute(db.inner())
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_cli_settings(db: State<'_, SqlitePool>, cli_type: String) -> Result<CliSettingsResponse> {
    let row = sqlx::query_as::<_, CliSettingsRow>(
        "SELECT cli_type, default_json_config, updated_at FROM cli_settings WHERE cli_type = ?",
    )
    .bind(&cli_type)
    .fetch_optional(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    if let Some(row) = row {
        // Check if CLI is enabled by reading config file
        let enabled = check_cli_enabled(&cli_type);
        Ok(CliSettingsResponse {
            cli_type: row.cli_type,
            enabled,
            default_json_config: row.default_json_config.unwrap_or_default(),
        })
    } else {
        Ok(CliSettingsResponse {
            cli_type,
            enabled: false,
            default_json_config: String::new(),
        })
    }
}

#[tauri::command]
pub async fn update_cli_settings(
    db: State<'_, SqlitePool>,
    cli_type: String,
    input: CliSettingsUpdate,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();

    // Validate and update database
    if let Some(ref config) = input.default_json_config {
        let config_trimmed = config.trim();

        // Validate format if config is not empty
        if !config_trimmed.is_empty() {
            match cli_type.as_str() {
                "claude_code" | "gemini" => {
                    // Validate JSON format
                    serde_json::from_str::<serde_json::Value>(config_trimmed)
                        .map_err(|e| format!("JSON 格式错误: {}", e))?;
                }
                "codex" => {
                    // Validate TOML format
                    config_trimmed.parse::<toml_edit::DocumentMut>()
                        .map_err(|e| format!("TOML 格式错误: {}", e))?;
                }
                _ => {}
            }
        }

        sqlx::query(
            "UPDATE cli_settings SET default_json_config = ?, updated_at = ? WHERE cli_type = ?",
        )
        .bind(config_trimmed)
        .bind(now)
        .bind(&cli_type)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    }

    // Update CLI config file if enabled flag is provided
    if let Some(enabled) = input.enabled {
        // Get default_json_config from database
        let row = sqlx::query_as::<_, CliSettingsRow>(
            "SELECT cli_type, default_json_config, updated_at FROM cli_settings WHERE cli_type = ?",
        )
        .bind(&cli_type)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| e.to_string())?;

        let default_config = row.and_then(|r| r.default_json_config).unwrap_or_default();
        sync_cli_config(&cli_type, enabled, &default_config, db).await?;
    }

    Ok(())
}

// Normalize text for comparison: trim, normalize whitespace, remove extra blank lines
fn normalize_text(text: &str) -> String {
    text.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("\n")
}

// Check if MCP config exists in the CLI config file
fn mcp_enabled_in_file(cli_type: &str, mcp_name: &str) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };

    match cli_type {
        "claude_code" => {
            let path = home.join(".claude.json");
            if !path.exists() {
                return false;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => return false,
            };
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(config) => {
                    config.get("mcpServers")
                        .and_then(|v| v.as_object())
                        .map(|servers| servers.contains_key(mcp_name))
                        .unwrap_or(false)
                }
                Err(_) => false,
            }
        }
        "gemini" => {
            let path = home.join(".gemini").join("settings.json");
            if !path.exists() {
                return false;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => return false,
            };
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(config) => {
                    config.get("mcpServers")
                        .and_then(|v| v.as_object())
                        .map(|servers| servers.contains_key(mcp_name))
                        .unwrap_or(false)
                }
                Err(_) => false,
            }
        }
        "codex" => {
            let path = home.join(".codex").join("config.toml");
            if !path.exists() {
                return false;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => return false,
            };
            match content.parse::<toml_edit::DocumentMut>() {
                Ok(doc) => {
                    doc.get("mcp_servers")
                        .and_then(|v| v.as_table())
                        .map(|servers| servers.contains_key(mcp_name))
                        .unwrap_or(false)
                }
                Err(_) => false,
            }
        }
        _ => false,
    }
}

// Check if prompt content matches the file content
fn prompt_enabled_in_file(cli_type: &str, prompt_content: &str) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };

    let prompt_path = match cli_type {
        "claude_code" => home.join(".claude").join("CLAUDE.md"),
        "codex" => home.join(".codex").join("AGENTS.md"),
        "gemini" => home.join(".gemini").join("GEMINI.md"),
        _ => return false,
    };

    if !prompt_path.exists() {
        return false;
    }

    let file_content = match std::fs::read_to_string(&prompt_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Normalize and compare
    normalize_text(prompt_content) == normalize_text(&file_content)
}

fn check_cli_enabled(cli_type: &str) -> bool {
    match cli_type {
        "claude_code" => check_claude_uses_gateway(),
        "codex" => check_codex_uses_gateway(),
        "gemini" => check_gemini_uses_gateway(),
        _ => false,
    }
}

fn check_claude_uses_gateway() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let config_path = home.join(".claude").join("settings.json");

    if !config_path.exists() {
        return false;
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let content_trimmed = content.trim();
    if content_trimmed.is_empty() || content_trimmed == "{}" {
        return false;
    }

    match serde_json::from_str::<serde_json::Value>(content_trimmed) {
        Ok(data) => {
            if let Some(env) = data.get("env") {
                if let Some(base_url) = env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()) {
                    return base_url.contains("127.0.0.1:7788") || base_url.contains("localhost:7788");
                }
            }
            false
        }
        Err(_) => false,
    }
}

fn check_codex_uses_gateway() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let config_path = home.join(".codex").join("config.toml");

    if !config_path.exists() {
        return false;
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    if content.trim().is_empty() {
        return false;
    }

    match content.parse::<toml_edit::DocumentMut>() {
        Ok(doc) => {
            // Check if model_provider is "ccg-gateway"
            if let Some(provider) = doc.get("model_provider").and_then(|v| v.as_str()) {
                if provider == "ccg-gateway" {
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

fn check_gemini_uses_gateway() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let env_path = home.join(".gemini").join(".env");

    if !env_path.exists() {
        return false;
    }

    let content = match std::fs::read_to_string(&env_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Check if .env contains GOOGLE_GEMINI_BASE_URL pointing to gateway
    for line in content.lines() {
        if line.starts_with("GOOGLE_GEMINI_BASE_URL=") {
            let url = line.split('=').nth(1).unwrap_or("");
            return url.contains("127.0.0.1:7788") || url.contains("localhost:7788");
        }
    }
    false
}

// Get the config file path for MCP/prompts sync (different for Codex)
fn get_mcp_config_path(cli_type: &str) -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    match cli_type {
        "claude_code" => Some(home.join(".claude.json")),  // Claude Code MCP goes to ~/.claude.json
        "codex" => Some(home.join(".codex").join("config.toml")),  // Codex MCP goes to config.toml
        "gemini" => Some(home.join(".gemini").join("settings.json")),
        _ => None,
    }
}

async fn sync_cli_config(cli_type: &str, enabled: bool, default_config: &str, db: State<'_, SqlitePool>) -> Result<()> {
    match cli_type {
        "claude_code" => sync_claude_code_config(enabled, default_config, db).await,
        "codex" => sync_codex_config(enabled, default_config, db).await,
        "gemini" => sync_gemini_config(enabled, default_config, db).await,
        _ => Err("Invalid CLI type".to_string()),
    }
}

fn get_backup_path(original_path: &std::path::Path) -> std::path::PathBuf {
    let file_name = original_path.file_name().unwrap().to_str().unwrap();
    original_path.parent().unwrap().join(format!("{}.ccg-backup", file_name))
}

fn backup_file(path: &std::path::Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let backup_path = get_backup_path(path);
    std::fs::copy(path, &backup_path).map_err(|e| {
        tracing::error!("Failed to backup {}: {}", path.display(), e);
        e.to_string()
    })?;
    Ok(())
}

fn restore_backup(path: &std::path::Path) -> Result<bool> {
    let backup_path = get_backup_path(path);
    if !backup_path.exists() {
        return Ok(false);
    }
    std::fs::copy(&backup_path, path).map_err(|e| {
        tracing::error!("Failed to restore backup from {}: {}", backup_path.display(), e);
        e.to_string()
    })?;
    std::fs::remove_file(&backup_path).map_err(|e| {
        tracing::warn!("Failed to remove backup file {}: {}", backup_path.display(), e);
        e.to_string()
    })?;
    Ok(true)
}

fn has_backup(path: &std::path::Path) -> bool {
    get_backup_path(path).exists()
}

fn deep_merge(base: &mut serde_json::Value, override_val: &serde_json::Value) {
    if let (Some(base_obj), Some(override_obj)) = (base.as_object_mut(), override_val.as_object()) {
        for (key, value) in override_obj {
            if let Some(base_value) = base_obj.get_mut(key) {
                if base_value.is_object() && value.is_object() {
                    deep_merge(base_value, value);
                } else {
                    *base_value = value.clone();
                }
            } else {
                base_obj.insert(key.clone(), value.clone());
            }
        }
    }
}

// Sync Claude Code configuration (settings.json)
async fn sync_claude_code_config(enabled: bool, default_config: &str, _db: State<'_, SqlitePool>) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot get home directory".to_string())?;
    let config_path = home.join(".claude").join("settings.json");

    if enabled {
        // Backup existing config if not already backed up
        if config_path.exists() && !has_backup(&config_path) {
            backup_file(&config_path)?;
        }

        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                tracing::error!("Failed to create directory: {}", e);
                e.to_string()
            })?;
        }

        // Build base config with gateway address
        let mut config = serde_json::json!({
            "env": {
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:7788",
                "ANTHROPIC_AUTH_TOKEN": "ccg-gateway"
            }
        });

        // Merge user's custom config if provided
        if !default_config.is_empty() {
            match serde_json::from_str::<serde_json::Value>(default_config) {
                Ok(custom_config) => {
                    deep_merge(&mut config, &custom_config);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse custom config (invalid JSON): {}", e);
                }
            }
        }

        // Write config file
        let config_str = serde_json::to_string_pretty(&config).map_err(|e| {
            tracing::error!("Failed to serialize config: {}", e);
            e.to_string()
        })?;
        std::fs::write(&config_path, config_str).map_err(|e| {
            tracing::error!("Failed to write config file: {}", e);
            e.to_string()
        })?;
    } else {
        // When disabling, restore backup or remove config file
        if restore_backup(&config_path)? {
        } else if config_path.exists() {
            // No backup, remove the config file
            std::fs::remove_file(&config_path).map_err(|e| {
                tracing::error!("Failed to remove config file: {}", e);
                e.to_string()
            })?;
        }
    }

    Ok(())
}

// Sync Codex configuration (auth.json + config.toml)
async fn sync_codex_config(enabled: bool, default_config: &str, _db: State<'_, SqlitePool>) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot get home directory".to_string())?;
    let codex_dir = home.join(".codex");
    let auth_path = codex_dir.join("auth.json");
    let config_path = codex_dir.join("config.toml");

    if enabled {
        // Backup existing configs if not already backed up
        if auth_path.exists() && !has_backup(&auth_path) {
            backup_file(&auth_path)?;
        }
        if config_path.exists() && !has_backup(&config_path) {
            backup_file(&config_path)?;
        }

        // Create config directory if it doesn't exist
        std::fs::create_dir_all(&codex_dir).map_err(|e| {
            tracing::error!("Failed to create Codex directory: {}", e);
            e.to_string()
        })?;

        // Write auth.json with gateway API key
        let auth = serde_json::json!({
            "OPENAI_API_KEY": "ccg-gateway"
        });
        let auth_str = serde_json::to_string_pretty(&auth).map_err(|e| {
            tracing::error!("Failed to serialize auth.json: {}", e);
            e.to_string()
        })?;
        std::fs::write(&auth_path, auth_str).map_err(|e| {
            tracing::error!("Failed to write auth.json: {}", e);
            e.to_string()
        })?;

        // Build base config.toml pointing to gateway
        let mut doc = toml_edit::DocumentMut::new();
        doc["model_provider"] = toml_edit::value("ccg-gateway");

        if !doc.contains_table("model_providers") {
            doc["model_providers"] = toml_edit::table();
        }

        let mut gateway_table = toml_edit::Table::new();
        gateway_table.insert("name", toml_edit::value("ccg-gateway"));
        gateway_table.insert("base_url", toml_edit::value("http://127.0.0.1:7788"));
        gateway_table.insert("wire_api", toml_edit::value("responses"));
        gateway_table.insert("requires_openai_auth", toml_edit::value(false));

        doc["model_providers"]["ccg-gateway"] = toml_edit::Item::Table(gateway_table);

        // Merge user's custom config if provided (TOML format)
        if !default_config.is_empty() {
            match default_config.parse::<toml_edit::DocumentMut>() {
                Ok(custom_doc) => {
                    // Merge custom config into base config
                    for (key, value) in custom_doc.iter() {
                        if key != "model_provider" && key != "model_providers" {
                            doc[key] = value.clone();
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse custom config (invalid TOML): {}", e);
                }
            }
        }

        std::fs::write(&config_path, doc.to_string()).map_err(|e| {
            tracing::error!("Failed to write config.toml: {}", e);
            e.to_string()
        })?;
    } else {
        // When disabling, restore backups or remove config files
        let auth_restored = restore_backup(&auth_path)?;
        let config_restored = restore_backup(&config_path)?;

        if auth_restored {
        } else if auth_path.exists() {
            std::fs::remove_file(&auth_path).map_err(|e| {
                tracing::error!("Failed to remove auth.json: {}", e);
                e.to_string()
            })?;
        }

        if config_restored {
        } else if config_path.exists() {
            std::fs::remove_file(&config_path).map_err(|e| {
                tracing::error!("Failed to remove config.toml: {}", e);
                e.to_string()
            })?;
        }
    }

    Ok(())
}

// Sync Gemini configuration (settings.json + .env)
async fn sync_gemini_config(enabled: bool, default_config: &str, _db: State<'_, SqlitePool>) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot get home directory".to_string())?;
    let gemini_dir = home.join(".gemini");
    let config_path = gemini_dir.join("settings.json");
    let env_path = gemini_dir.join(".env");

    if enabled {
        // Backup existing configs if not already backed up
        if config_path.exists() && !has_backup(&config_path) {
            backup_file(&config_path)?;
        }
        if env_path.exists() && !has_backup(&env_path) {
            backup_file(&env_path)?;
        }

        // Create config directory if it doesn't exist
        std::fs::create_dir_all(&gemini_dir).map_err(|e| {
            tracing::error!("Failed to create Gemini directory: {}", e);
            e.to_string()
        })?;

        // Write .env file with gateway address
        let env_content = "GEMINI_API_KEY=ccg-gateway\nGOOGLE_GEMINI_BASE_URL=http://127.0.0.1:7788\n".to_string();
        std::fs::write(&env_path, env_content).map_err(|e| {
            tracing::error!("Failed to write .env file: {}", e);
            e.to_string()
        })?;

        // Build base config with security.auth.selectedType
        let mut config = serde_json::json!({
            "security": {
                "auth": {
                    "selectedType": "gemini-api-key"
                }
            }
        });

        // Merge user's custom config if provided
        if !default_config.is_empty() {
            match serde_json::from_str::<serde_json::Value>(default_config) {
                Ok(custom_config) => {
                    deep_merge(&mut config, &custom_config);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse custom config (invalid JSON): {}", e);
                }
            }
        }

        // Write config file
        let config_str = serde_json::to_string_pretty(&config).map_err(|e| {
            tracing::error!("Failed to serialize config.json: {}", e);
            e.to_string()
        })?;
        std::fs::write(&config_path, config_str).map_err(|e| {
            tracing::error!("Failed to write config.json: {}", e);
            e.to_string()
        })?;
    } else {
        // When disabling, restore backups or remove config files
        let env_restored = restore_backup(&env_path)?;
        let config_restored = restore_backup(&config_path)?;

        if env_restored {
        } else if env_path.exists() {
            std::fs::remove_file(&env_path).map_err(|e| {
                tracing::error!("Failed to remove .env file: {}", e);
                e.to_string()
            })?;
        }

        if config_restored {
        } else if config_path.exists() {
            std::fs::remove_file(&config_path).map_err(|e| {
                tracing::error!("Failed to remove config.json: {}", e);
                e.to_string()
            })?;
        }
    }

    Ok(())
}

// Log commands
#[tauri::command]
pub async fn get_request_logs(
    log_db: State<'_, crate::LogDb>,
    page: Option<i64>,
    page_size: Option<i64>,
    cli_type: Option<String>,
) -> Result<PaginatedLogs> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * page_size;
    let pool = &log_db.0;

    let (items, total) = if let Some(ct) = cli_type {
        let items = sqlx::query_as::<_, RequestLogItem>(
            "SELECT id, created_at, cli_type, provider_name, model_id, status_code, elapsed_ms, input_tokens, output_tokens, client_method, client_path FROM request_logs WHERE cli_type = ? ORDER BY id DESC LIMIT ? OFFSET ?",
        )
        .bind(&ct)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs WHERE cli_type = ?")
            .bind(&ct)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;

        (items, total.0)
    } else {
        let items = sqlx::query_as::<_, RequestLogItem>(
            "SELECT id, created_at, cli_type, provider_name, model_id, status_code, elapsed_ms, input_tokens, output_tokens, client_method, client_path FROM request_logs ORDER BY id DESC LIMIT ? OFFSET ?",
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs")
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;

        (items, total.0)
    };

    Ok(PaginatedLogs {
        items,
        total,
        page,
        page_size,
    })
}

#[tauri::command]
pub async fn clear_request_logs(log_db: State<'_, crate::LogDb>) -> Result<()> {
    sqlx::query("DELETE FROM request_logs")
        .execute(&log_db.0)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_request_log_detail(
    log_db: State<'_, crate::LogDb>,
    id: i64,
) -> Result<RequestLogDetail> {
    sqlx::query_as::<_, RequestLogDetail>(
        "SELECT id, created_at, cli_type, provider_name, model_id, status_code, elapsed_ms, input_tokens, output_tokens, client_method, client_path, client_headers, client_body, forward_url, forward_headers, forward_body, provider_headers, provider_body, response_headers, response_body, error_message FROM request_logs WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&log_db.0)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Log not found".to_string())
}

// System logs commands
#[tauri::command]
pub async fn get_system_logs(
    log_db: State<'_, crate::LogDb>,
    page: Option<i64>,
    page_size: Option<i64>,
    level: Option<String>,
    event_type: Option<String>,
    provider_name: Option<String>,
) -> Result<SystemLogListResponse> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * page_size;

    // Build query
    let mut sql = "SELECT * FROM system_logs WHERE 1=1".to_string();
    let mut count_sql = "SELECT COUNT(*) FROM system_logs WHERE 1=1".to_string();

    if level.is_some() {
        sql.push_str(" AND level = ?");
        count_sql.push_str(" AND level = ?");
    }
    if event_type.is_some() {
        sql.push_str(" AND event_type = ?");
        count_sql.push_str(" AND event_type = ?");
    }
    if provider_name.is_some() {
        sql.push_str(" AND provider_name = ?");
        count_sql.push_str(" AND provider_name = ?");
    }

    sql.push_str(" ORDER BY id DESC LIMIT ? OFFSET ?");
    let mut q = sqlx::query_as::<_, SystemLogItem>(&sql)
        .bind(page_size)
        .bind(offset);

    if let Some(ref lvl) = level {
        q = q.bind(lvl);
    }
    if let Some(ref et) = event_type {
        q = q.bind(et);
    }
    if let Some(ref pn) = provider_name {
        q = q.bind(pn);
    }

    let items = q.fetch_all(&log_db.0)
        .await
        .map_err(|e| e.to_string())?;

    // Get total count
    let mut count_q = sqlx::query_as::<_, (i64,)>(&count_sql);
    if let Some(ref lvl) = level {
        count_q = count_q.bind(lvl);
    }
    if let Some(ref et) = event_type {
        count_q = count_q.bind(et);
    }
    if let Some(ref pn) = provider_name {
        count_q = count_q.bind(pn);
    }
    let (total,) = count_q.fetch_one(&log_db.0)
        .await
        .map_err(|e| e.to_string())?;

    Ok(SystemLogListResponse {
        items,
        total,
        page,
        page_size,
    })
}

#[tauri::command]
pub async fn clear_system_logs(log_db: State<'_, crate::LogDb>) -> Result<()> {
    sqlx::query("DELETE FROM system_logs")
        .execute(&log_db.0)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// System status
#[tauri::command]
pub async fn get_system_status(start_time: State<'_, crate::StartTime>) -> Result<SystemStatus> {
    let uptime = chrono::Utc::now().timestamp() - start_time.0;
    Ok(SystemStatus {
        status: "running".to_string(),
        port: 7788,
        uptime,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

// MCP commands
#[tauri::command]
pub async fn get_mcps(db: State<'_, SqlitePool>) -> Result<Vec<McpResponse>> {
    let mcps = sqlx::query_as::<_, McpConfig>("SELECT * FROM mcp_configs ORDER BY id")
        .fetch_all(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    let cli_types = vec!["claude_code", "codex", "gemini"];

    let mut results = Vec::new();
    for mcp in mcps {
        // Read real status from config files
        let mut cli_flags = Vec::new();
        for cli_type in &cli_types {
            let enabled = mcp_enabled_in_file(cli_type, &mcp.name);
            cli_flags.push(McpCliFlag {
                cli_type: cli_type.to_string(),
                enabled,
            });
        }

        results.push(McpResponse {
            id: mcp.id,
            name: mcp.name,
            config_json: mcp.config_json,
            cli_flags,
        });
    }
    Ok(results)
}

#[tauri::command]
pub async fn get_mcp(db: State<'_, SqlitePool>, id: i64) -> Result<McpResponse> {
    let mcp = sqlx::query_as::<_, McpConfig>("SELECT * FROM mcp_configs WHERE id = ?")
        .bind(id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "MCP not found".to_string())?;

    // Read real status from config files
    let cli_types = vec!["claude_code", "codex", "gemini"];
    let mut cli_flags = Vec::new();
    for cli_type in &cli_types {
        let enabled = mcp_enabled_in_file(cli_type, &mcp.name);
        cli_flags.push(McpCliFlag {
            cli_type: cli_type.to_string(),
            enabled,
        });
    }

    Ok(McpResponse {
        id: mcp.id,
        name: mcp.name,
        config_json: mcp.config_json,
        cli_flags,
    })
}

#[tauri::command]
pub async fn create_mcp(db: State<'_, SqlitePool>, input: McpCreate) -> Result<McpResponse> {
    let now = chrono::Utc::now().timestamp();

    let result = sqlx::query(
        "INSERT INTO mcp_configs (name, config_json, updated_at) VALUES (?, ?, ?)",
    )
    .bind(&input.name)
    .bind(&input.config_json)
    .bind(now)
    .execute(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    let id = result.last_insert_rowid();

    // Sync to CLI files if cli_flags provided
    let cli_flags = input.cli_flags.unwrap_or_default();
    if !cli_flags.is_empty() {
        sync_single_mcp_to_cli(id, &input.name, &input.config_json, &cli_flags).await?;
    }

    get_mcp(db, id).await
}

#[tauri::command]
pub async fn update_mcp(db: State<'_, SqlitePool>, id: i64, input: McpUpdate) -> Result<McpResponse> {
    let now = chrono::Utc::now().timestamp();

    let (name, config_json) = if input.name.is_some() || input.config_json.is_some() {
        let current = sqlx::query_as::<_, McpConfig>("SELECT * FROM mcp_configs WHERE id = ?")
            .bind(id)
            .fetch_optional(db.inner())
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "MCP not found".to_string())?;

        let new_name = input.name.unwrap_or(current.name.clone());
        let new_config = input.config_json.unwrap_or(current.config_json.clone());

        sqlx::query(
            "UPDATE mcp_configs SET name = ?, config_json = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&new_name)
        .bind(&new_config)
        .bind(now)
        .bind(id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

        (new_name, new_config)
    } else {
        // Get current values if not updating
        let current = sqlx::query_as::<_, McpConfig>("SELECT * FROM mcp_configs WHERE id = ?")
            .bind(id)
            .fetch_optional(db.inner())
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "MCP not found".to_string())?;
        (current.name, current.config_json)
    };

    // Sync to CLI files if cli_flags provided
    if let Some(cli_flags) = input.cli_flags {
        sync_single_mcp_to_cli(id, &name, &config_json, &cli_flags).await?;
    }

    get_mcp(db, id).await
}

#[tauri::command]
pub async fn delete_mcp(db: State<'_, SqlitePool>, id: i64) -> Result<()> {
    // Get MCP name before deletion
    let mcp = sqlx::query_as::<_, McpConfig>("SELECT * FROM mcp_configs WHERE id = ?")
        .bind(id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "MCP not found".to_string())?;

    let mcp_name = mcp.name.clone();

    // Delete from database
    sqlx::query("DELETE FROM mcp_configs WHERE id = ?")
        .bind(id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    // Remove from all CLI configs
    delete_mcp_from_cli(&mcp_name)?;

    Ok(())
}

// Sync a single MCP to CLI files based on enabled flags
async fn sync_single_mcp_to_cli(
    _mcp_id: i64,
    mcp_name: &str,
    mcp_config_json: &str,
    cli_flags: &[McpCliFlag],
) -> Result<()> {
    let cli_types = vec!["claude_code", "codex", "gemini"];

    for cli_type in cli_types {
        // Check if this MCP is enabled for this CLI
        let is_enabled = cli_flags.iter()
            .any(|f| f.cli_type == cli_type && f.enabled);

        let config_path = get_mcp_config_path(cli_type);
        if let Some(path) = config_path {
            // Handle Codex separately (TOML format)
            if cli_type == "codex" {
                sync_single_codex_mcp(path, mcp_name, mcp_config_json, is_enabled)?;
                continue;
            }

            // For ClaudeCode and Gemini (JSON format)
            // Read existing config or create new one
            let mut config = if path.exists() {
                let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            // Update MCP section
            if is_enabled {
                // Add or update this MCP
                if let Ok(mcp_json) = serde_json::from_str::<serde_json::Value>(mcp_config_json) {
                    if let Some(obj) = config.as_object_mut() {
                        if !obj.contains_key("mcpServers") {
                            obj.insert("mcpServers".to_string(), serde_json::json!({}));
                        }
                        if let Some(servers) = obj.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
                            servers.insert(mcp_name.to_string(), mcp_json);
                        }
                    }
                }
            } else {
                // Remove this MCP by name
                if let Some(obj) = config.as_object_mut() {
                    if let Some(servers) = obj.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
                        servers.remove(mcp_name);
                    }
                }
            }

            // Write config file
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let config_str = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
            std::fs::write(&path, config_str).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

// Helper function to sync a single MCP to Codex config.toml
fn sync_single_codex_mcp(
    config_path: std::path::PathBuf,
    mcp_name: &str,
    mcp_config_json: &str,
    is_enabled: bool,
) -> Result<()> {
    // Read existing TOML or create new one
    let mut doc = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).map_err(|e| {
            tracing::error!("Failed to read config.toml: {}", e);
            e.to_string()
        })?;
        content.parse::<toml_edit::DocumentMut>().unwrap_or_else(|e| {
            tracing::warn!("Failed to parse config.toml, creating new: {}", e);
            toml_edit::DocumentMut::new()
        })
    } else {
        toml_edit::DocumentMut::new()
    };

    // Ensure mcp_servers table exists
    if !doc.contains_table("mcp_servers") {
        doc["mcp_servers"] = toml_edit::table();
    }

    if is_enabled {
        // Add or update this MCP
        if let Ok(mcp_config) = serde_json::from_str::<serde_json::Value>(mcp_config_json) {
            let mcp_type = mcp_config.get("type").and_then(|v| v.as_str()).unwrap_or("stdio");

            // Create MCP server table
            let mut server_table = toml_edit::Table::new();

            // Handle STDIO type servers
            if let Some(command) = mcp_config.get("command").and_then(|v| v.as_str()) {
                server_table.insert("command", toml_edit::value(command));
            }
            if let Some(args) = mcp_config.get("args").and_then(|v| v.as_array()) {
                let args_array: toml_edit::Array = args.iter()
                    .filter_map(|v| v.as_str())
                    .map(toml_edit::Value::from)
                    .collect();
                server_table.insert("args", toml_edit::Item::Value(args_array.into()));
            }
            if let Some(env) = mcp_config.get("env").and_then(|v| v.as_object()) {
                let mut env_table = toml_edit::Table::new();
                for (k, v) in env.iter() {
                    if let Some(v_str) = v.as_str() {
                        env_table.insert(k, toml_edit::value(v_str));
                    }
                }
                server_table.insert("env", toml_edit::Item::Table(env_table));
            }
            if let Some(cwd) = mcp_config.get("cwd").and_then(|v| v.as_str()) {
                server_table.insert("cwd", toml_edit::value(cwd));
            }

            // Handle HTTP/SSE type servers
            if mcp_type == "sse" || mcp_type == "http" {
                if let Some(url) = mcp_config.get("url").and_then(|v| v.as_str()) {
                    server_table.insert("url", toml_edit::value(url));
                }
            }

            // Optional fields
            if let Some(timeout) = mcp_config.get("startup_timeout_sec").and_then(|v| v.as_i64()) {
                server_table.insert("startup_timeout_sec", toml_edit::value(timeout));
            }
            if let Some(timeout) = mcp_config.get("tool_timeout_sec").and_then(|v| v.as_i64()) {
                server_table.insert("tool_timeout_sec", toml_edit::value(timeout));
            }

            doc["mcp_servers"][mcp_name] = toml_edit::Item::Table(server_table);
        }
    } else {
        // Remove this MCP by name
        if let Some(table) = doc.get_mut("mcp_servers").and_then(|v| v.as_table_mut()) {
            table.remove(mcp_name);
        }
    }

    // Write config file
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            tracing::error!("Failed to create directory: {}", e);
            e.to_string()
        })?;
    }
    std::fs::write(&config_path, doc.to_string()).map_err(|e| {
        tracing::error!("Failed to write config.toml: {}", e);
        e.to_string()
    })?;

    Ok(())
}

// Delete a single MCP from all CLI configs
fn delete_mcp_from_cli(mcp_name: &str) -> Result<()> {
    let cli_types = vec!["claude_code", "codex", "gemini"];

    for cli_type in cli_types {
        let config_path = get_mcp_config_path(cli_type);
        if let Some(path) = config_path {
            if !path.exists() {
                continue;
            }

            if cli_type == "codex" {
                // Handle Codex TOML format
                let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                let mut doc = content.parse::<toml_edit::DocumentMut>().unwrap_or_else(|_| toml_edit::DocumentMut::new());

                if let Some(table) = doc["mcp_servers"].as_table_mut() {
                    table.remove(mcp_name);
                }

                std::fs::write(&path, doc.to_string()).map_err(|e| e.to_string())?;
            } else {
                // Handle Claude/Gemini JSON format
                let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                let mut config: serde_json::Value = serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}));

                if let Some(mcp_servers) = config.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
                    mcp_servers.remove(mcp_name);
                }

                let config_str = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
                std::fs::write(&path, config_str).map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

// Prompt commands
#[tauri::command]
pub async fn get_prompts(db: State<'_, SqlitePool>) -> Result<Vec<PromptResponse>> {
    let prompts = sqlx::query_as::<_, PromptPreset>("SELECT * FROM prompt_presets ORDER BY id")
        .fetch_all(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    let cli_types = vec!["claude_code", "codex", "gemini"];

    let mut results = Vec::new();
    for prompt in prompts {
        // Read real status from prompt files
        let mut cli_flags = Vec::new();
        for cli_type in &cli_types {
            let enabled = prompt_enabled_in_file(cli_type, &prompt.content);
            cli_flags.push(PromptCliFlag {
                cli_type: cli_type.to_string(),
                enabled,
            });
        }

        results.push(PromptResponse {
            id: prompt.id,
            name: prompt.name,
            content: prompt.content,
            cli_flags,
        });
    }
    Ok(results)
}

#[tauri::command]
pub async fn get_prompt(db: State<'_, SqlitePool>, id: i64) -> Result<PromptResponse> {
    let prompt = sqlx::query_as::<_, PromptPreset>("SELECT * FROM prompt_presets WHERE id = ?")
        .bind(id)
        .fetch_optional(db.inner())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Prompt not found".to_string())?;

    // Read real status from prompt files
    let cli_types = vec!["claude_code", "codex", "gemini"];
    let mut cli_flags = Vec::new();
    for cli_type in &cli_types {
        let enabled = prompt_enabled_in_file(cli_type, &prompt.content);
        cli_flags.push(PromptCliFlag {
            cli_type: cli_type.to_string(),
            enabled,
        });
    }

    Ok(PromptResponse {
        id: prompt.id,
        name: prompt.name,
        content: prompt.content,
        cli_flags,
    })
}

#[tauri::command]
pub async fn create_prompt(db: State<'_, SqlitePool>, input: PromptCreate) -> Result<PromptResponse> {
    let now = chrono::Utc::now().timestamp();

    let result = sqlx::query(
        "INSERT INTO prompt_presets (name, content, updated_at) VALUES (?, ?, ?)",
    )
    .bind(&input.name)
    .bind(&input.content)
    .bind(now)
    .execute(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    let id = result.last_insert_rowid();

    // Sync to CLI files if cli_flags provided
    let cli_flags = input.cli_flags.unwrap_or_default();
    if !cli_flags.is_empty() {
        sync_single_prompt_to_cli(&input.content, &cli_flags).await?;
    }

    get_prompt(db, id).await
}

#[tauri::command]
pub async fn update_prompt(db: State<'_, SqlitePool>, id: i64, input: PromptUpdate) -> Result<PromptResponse> {
    let now = chrono::Utc::now().timestamp();

    let content = if input.name.is_some() || input.content.is_some() {
        let current = sqlx::query_as::<_, PromptPreset>("SELECT * FROM prompt_presets WHERE id = ?")
            .bind(id)
            .fetch_optional(db.inner())
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Prompt not found".to_string())?;

        let new_name = input.name.unwrap_or(current.name.clone());
        let new_content = input.content.unwrap_or(current.content.clone());

        sqlx::query(
            "UPDATE prompt_presets SET name = ?, content = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&new_name)
        .bind(&new_content)
        .bind(now)
        .bind(id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

        new_content
    } else {
        // Get current values if not updating
        let current = sqlx::query_as::<_, PromptPreset>("SELECT * FROM prompt_presets WHERE id = ?")
            .bind(id)
            .fetch_optional(db.inner())
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Prompt not found".to_string())?;
        current.content
    };

    // Sync to CLI files if cli_flags provided
    if let Some(cli_flags) = input.cli_flags {
        sync_single_prompt_to_cli(&content, &cli_flags).await?;
    }

    get_prompt(db, id).await
}

#[tauri::command]
pub async fn delete_prompt(db: State<'_, SqlitePool>, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM prompt_presets WHERE id = ?")
        .bind(id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    // Sync prompt configs to CLI files
    sync_prompt_configs_to_cli(db).await?;

    Ok(())
}

// Sync a single prompt to CLI files based on enabled flags
async fn sync_single_prompt_to_cli(
    prompt_content: &str,
    cli_flags: &[PromptCliFlag],
) -> Result<()> {
    let cli_types = vec!["claude_code", "codex", "gemini"];

    for cli_type in cli_types {
        // Check if this prompt is enabled for this CLI
        let is_enabled = cli_flags.iter()
            .any(|f| f.cli_type == cli_type && f.enabled);

        // Get the prompt file path for this CLI
        let prompt_path = get_prompt_file_path(cli_type);
        if let Some(path) = prompt_path {
            // Check if CLI directory exists (skip if CLI not installed)
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    continue;
                }

                if is_enabled {
                    // Write prompt content to file
                    std::fs::write(&path, prompt_content).map_err(|e| {
                        tracing::error!("Failed to write prompt file: {}", e);
                        e.to_string()
                    })?;
                } else {
                    // Check if this prompt was previously in the file
                    if path.exists() {
                        let file_content = std::fs::read_to_string(&path).unwrap_or_default();
                        if normalize_text(prompt_content) == normalize_text(&file_content) {
                            // This prompt was in the file, clear it
                            std::fs::write(&path, "").map_err(|e| {
                                tracing::error!("Failed to clear prompt file: {}", e);
                                e.to_string()
                            })?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn sync_prompt_configs_to_cli(_db: State<'_, SqlitePool>) -> Result<()> {
    // This function is no longer used, keeping for compatibility
    Ok(())
}

fn get_prompt_file_path(cli_type: &str) -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    match cli_type {
        "claude_code" => Some(home.join(".claude").join("CLAUDE.md")),
        "codex" => Some(home.join(".codex").join("AGENTS.md")),
        "gemini" => Some(home.join(".gemini").join("GEMINI.md")),
        _ => None,
    }
}

// Stats commands
#[tauri::command]
pub async fn get_daily_stats(
    log_db: State<'_, crate::LogDb>,
    start_date: Option<String>,
    end_date: Option<String>,
    cli_type: Option<String>,
) -> Result<Vec<DailyStats>> {
    let pool = &log_db.0;

    let mut query = "SELECT * FROM usage_daily WHERE 1=1".to_string();
    if start_date.is_some() {
        query.push_str(" AND usage_date >= ?");
    }
    if end_date.is_some() {
        query.push_str(" AND usage_date <= ?");
    }
    if cli_type.is_some() {
        query.push_str(" AND cli_type = ?");
    }
    query.push_str(" ORDER BY usage_date DESC");

    let mut q = sqlx::query_as::<_, DailyStats>(&query);
    if let Some(ref sd) = start_date {
        q = q.bind(sd);
    }
    if let Some(ref ed) = end_date {
        q = q.bind(ed);
    }
    if let Some(ref ct) = cli_type {
        q = q.bind(ct);
    }

    q.fetch_all(pool).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_provider_stats(
    log_db: State<'_, crate::LogDb>,
    start_date: Option<String>,
    end_date: Option<String>,
    cli_type: Option<String>,
    provider_name: Option<String>,
) -> Result<Vec<ProviderStatsResponse>> {
    let pool = &log_db.0;

    let mut query = r#"
        SELECT
            cli_type,
            provider_name,
            model_id,
            COUNT(*) as total_requests,
            SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END) as total_success,
            SUM(input_tokens + output_tokens) as total_tokens,
            SUM(elapsed_ms) as total_elapsed_ms
        FROM request_logs
        WHERE 1=1
    "#.to_string();

    if start_date.is_some() {
        query.push_str(" AND datetime(created_at, 'unixepoch', 'localtime') >= ?");
    }
    if end_date.is_some() {
        query.push_str(" AND datetime(created_at, 'unixepoch', 'localtime') <= ?");
    }
    if cli_type.is_some() {
        query.push_str(" AND cli_type = ?");
    }
    if provider_name.is_some() {
        query.push_str(" AND provider_name = ?");
    }
    query.push_str(" GROUP BY cli_type, provider_name, model_id ORDER BY total_requests DESC");

    let mut q = sqlx::query_as::<_, ProviderStatsRow>(&query);
    if let Some(ref sd) = start_date {
        q = q.bind(sd);
    }
    if let Some(ref ed) = end_date {
        q = q.bind(ed);
    }
    if let Some(ref ct) = cli_type {
        q = q.bind(ct);
    }
    if let Some(ref pn) = provider_name {
        q = q.bind(pn);
    }

    let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;

    let results = rows.into_iter().map(|row| ProviderStatsResponse {
        cli_type: row.cli_type,
        provider_name: row.provider_name,
        model_id: row.model_id,
        total_requests: row.total_requests,
        total_success: row.total_success,
        total_tokens: row.total_tokens,
        total_elapsed_ms: row.total_elapsed_ms,
        success_rate: if row.total_requests > 0 {
            (row.total_success as f64 / row.total_requests as f64) * 100.0
        } else {
            0.0
        },
    }).collect();

    Ok(results)
}

// Session helpers
fn get_cli_base_dir(cli_type: &str) -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    match cli_type {
        "codex" => home.join(".codex"),
        "gemini" => home.join(".gemini"),
        _ => home.join(".claude"),
    }
}

// Extract cwd from Codex session file
fn extract_codex_cwd(file_path: &std::path::Path) -> Option<String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(file_path).ok()?;
    let reader = BufReader::new(file);
    
    for line in reader.lines().flatten() {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&line) {
            if data.get("type").and_then(|t| t.as_str()) == Some("session_meta") {
                if let Some(cwd) = data.get("payload")
                    .and_then(|p| p.get("cwd"))
                    .and_then(|c| c.as_str()) {
                    return Some(cwd.to_string());
                }
            }
        }
    }
    None
}

// Handle Codex projects (group sessions by cwd)
fn get_codex_projects(sessions_dir: std::path::PathBuf, page: i64, page_size: i64) -> Result<PaginatedProjects> {
    use std::collections::HashMap;
    use walkdir::WalkDir;
    
    if !sessions_dir.exists() {
        return Ok(PaginatedProjects {
            items: vec![],
            total: 0,
            page,
            page_size,
        });
    }
    
    // Group sessions by cwd (search recursively in date subdirectories)
    let mut project_map: HashMap<String, Vec<(std::path::PathBuf, std::fs::Metadata)>> = HashMap::new();
    
    // Use WalkDir to recursively search all subdirectories
    for entry in WalkDir::new(&sessions_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            if filename.starts_with("rollout-") && filename.ends_with(".jsonl") {
                if let Some(cwd) = extract_codex_cwd(path) {
                    if let Ok(meta) = path.metadata() {
                        project_map.entry(cwd).or_insert_with(Vec::new).push((path.to_path_buf(), meta));
                    }
                }
            }
        }
    }
    
    // Build project list
    let mut projects_data: Vec<(String, String, usize, i64, f64)> = Vec::new();
    for (cwd, files) in project_map {
        let total_size: i64 = files.iter().map(|(_, m)| m.len() as i64).sum();
        let last_modified = files.iter()
            .filter_map(|(_, m)| m.modified().ok())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0))
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.0);
        
        let display_name = std::path::Path::new(&cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();
        
        projects_data.push((cwd.clone(), display_name, files.len(), total_size, last_modified));
    }
    
    // Sort by last_modified descending
    projects_data.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal));
    
    let total = projects_data.len() as i64;
    let start = ((page - 1) * page_size) as usize;
    let items: Vec<_> = projects_data.into_iter()
        .skip(start)
        .take(page_size as usize)
        .map(|(cwd, display_name, session_count, total_size, last_modified)| ProjectInfo {
            name: cwd.clone(),
            display_name,
            full_path: cwd,
            session_count: session_count as i64,
            total_size,
            last_modified,
        })
        .collect();
    
    Ok(PaginatedProjects {
        items,
        total,
        page,
        page_size,
    })
}

// Handle Gemini projects (from hash directories with chats subfolder)
fn get_gemini_projects(tmp_dir: std::path::PathBuf, page: i64, page_size: i64) -> Result<PaginatedProjects> {
    if !tmp_dir.exists() {
        return Ok(PaginatedProjects {
            items: vec![],
            total: 0,
            page,
            page_size,
        });
    }
    
    let mut project_dirs: Vec<(std::path::PathBuf, f64)> = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            // Check if it's a valid 64-char hex hash
            if name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit()) {
                let chats_dir = path.join("chats");
                if chats_dir.exists() {
                    if let Ok(meta) = path.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            let secs = mtime.duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs_f64())
                                .unwrap_or(0.0);
                            project_dirs.push((path, secs));
                        }
                    }
                }
            }
        }
    }
    
    // Sort by last_modified descending
    project_dirs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    let total = project_dirs.len() as i64;
    let start = ((page - 1) * page_size) as usize;
    let page_dirs: Vec<_> = project_dirs.into_iter().skip(start).take(page_size as usize).collect();
    
    let mut projects = Vec::new();
    for (path, _) in page_dirs {
        let hash_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        
        let chats_dir = path.join("chats");
        let mut session_count = 0i64;
        let mut total_size = 0i64;
        let mut last_modified = 0f64;
        
        if let Ok(entries) = std::fs::read_dir(&chats_dir) {
            for entry in entries.flatten() {
                let session_path = entry.path();
                if session_path.is_file() {
                    let filename = session_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    
                    if filename.starts_with("session-") && filename.ends_with(".json") {
                        session_count += 1;
                        if let Ok(meta) = session_path.metadata() {
                            total_size += meta.len() as i64;
                            if let Ok(mtime) = meta.modified() {
                                let secs = mtime.duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs_f64())
                                    .unwrap_or(0.0);
                                if secs > last_modified {
                                    last_modified = secs;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if session_count > 0 {
            let display_name = format!("Project {}", &hash_name[..8]);
            projects.push(ProjectInfo {
                name: hash_name.to_string(),
                display_name,
                full_path: hash_name.to_string(),
                session_count,
                total_size,
                last_modified,
            });
        }
    }
    
    Ok(PaginatedProjects {
        items: projects,
        total,
        page,
        page_size,
    })
}

// Handle Codex sessions (find by cwd)
fn get_codex_sessions(project_name: &str, page: i64, page_size: i64) -> Result<PaginatedSessions> {
    use std::io::{BufRead, BufReader};
    use walkdir::WalkDir;
    
    let home = dirs::home_dir().unwrap_or_default();
    let sessions_dir = home.join(".codex").join("sessions");
    
    if !sessions_dir.exists() {
        return Ok(PaginatedSessions {
            items: vec![],
            total: 0,
            page,
            page_size,
        });
    }
    
    let mut session_files: Vec<(std::path::PathBuf, std::fs::Metadata)> = Vec::new();
    
    // Use WalkDir to recursively search all subdirectories
    for entry in WalkDir::new(&sessions_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            if filename.starts_with("rollout-") && filename.ends_with(".jsonl") {
                if let Some(cwd) = extract_codex_cwd(path) {
                    if cwd == project_name {
                        if let Ok(meta) = path.metadata() {
                            session_files.push((path.to_path_buf(), meta));
                        }
                    }
                }
            }
        }
    }
    
    // Sort by mtime descending
    session_files.sort_by(|a, b| {
        let a_mtime = a.1.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let b_mtime = b.1.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        b_mtime.partial_cmp(&a_mtime).unwrap_or(std::cmp::Ordering::Equal)
    });
    
    let total = session_files.len() as i64;
    let start = ((page - 1) * page_size) as usize;
    let page_files: Vec<_> = session_files.into_iter().skip(start).take(page_size as usize).collect();
    
    let mut sessions = Vec::new();
    for (path, meta) in page_files {
        let session_id = path.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        
        let size = meta.len() as i64;
        let mtime = meta.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        
        // Try to extract first message
        let mut first_message = String::new();
        if let Ok(file) = std::fs::File::open(&path) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&line) {
                    if data.get("type").and_then(|t| t.as_str()) == Some("event_msg") {
                        if let Some(payload) = data.get("payload") {
                            if payload.get("type").and_then(|t| t.as_str()) == Some("user_message") {
                                if let Some(msg) = payload.get("message").and_then(|m| m.as_str()) {
                                    first_message = msg.chars().take(200).collect();
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        sessions.push(SessionInfo {
            session_id,
            size,
            mtime,
            first_message,
            git_branch: String::new(),
            summary: String::new(),
        });
    }
    
    Ok(PaginatedSessions {
        items: sessions,
        total,
        page,
        page_size,
    })
}

// Handle Gemini sessions
fn get_gemini_sessions(project_name: &str, page: i64, page_size: i64) -> Result<PaginatedSessions> {
    let home = dirs::home_dir().unwrap_or_default();
    let chats_dir = home.join(".gemini").join("tmp").join(project_name).join("chats");
    
    if !chats_dir.exists() {
        return Ok(PaginatedSessions {
            items: vec![],
            total: 0,
            page,
            page_size,
        });
    }
    
    let mut session_files: Vec<(std::path::PathBuf, std::fs::Metadata)> = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(&chats_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                
                if filename.starts_with("session-") && filename.ends_with(".json") {
                    if let Ok(meta) = path.metadata() {
                        session_files.push((path, meta));
                    }
                }
            }
        }
    }
    
    // Sort by mtime descending
    session_files.sort_by(|a, b| {
        let a_mtime = a.1.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let b_mtime = b.1.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        b_mtime.partial_cmp(&a_mtime).unwrap_or(std::cmp::Ordering::Equal)
    });
    
    let total = session_files.len() as i64;
    let start = ((page - 1) * page_size) as usize;
    let page_files: Vec<_> = session_files.into_iter().skip(start).take(page_size as usize).collect();
    
    let mut sessions = Vec::new();
    for (path, meta) in page_files {
        let session_id = path.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        
        let size = meta.len() as i64;
        let mtime = meta.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        
        // Try to extract first message
        let mut first_message = String::new();
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(messages) = json.get("messages").and_then(|m| m.as_array()) {
                    for msg in messages {
                        if msg.get("type").and_then(|t| t.as_str()) == Some("user") {
                            if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                                first_message = text.chars().take(200).collect();
                                break;
                            }
                        }
                    }
                }
            }
        }
        
        sessions.push(SessionInfo {
            session_id,
            size,
            mtime,
            first_message,
            git_branch: String::new(),
            summary: String::new(),
        });
    }
    
    Ok(PaginatedSessions {
        items: sessions,
        total,
        page,
        page_size,
    })
}

// Parse Codex messages from JSONL file
fn get_codex_messages(session_id: &str) -> Result<Vec<SessionMessage>> {
    use std::io::{BufRead, BufReader};
    use walkdir::WalkDir;
    
    let home = dirs::home_dir().unwrap_or_default();
    let sessions_dir = home.join(".codex").join("sessions");
    
    // Find the session file by searching recursively
    let mut session_file_path: Option<std::path::PathBuf> = None;
    for entry in WalkDir::new(&sessions_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            // Match session_id which is the stem (filename without extension)
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem == session_id {
                    session_file_path = Some(path.to_path_buf());
                    break;
                }
            }
        }
    }
    
    let session_file = session_file_path.ok_or_else(|| format!("Session file not found: {}", session_id))?;
    
    let file = std::fs::File::open(&session_file)
        .map_err(|e| format!("Failed to open session file: {}", e))?;
    let reader = BufReader::new(file);
    
    let mut messages = Vec::new();
    
    for line in reader.lines().flatten() {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&line) {
            let msg_type = data.get("type").and_then(|t| t.as_str());
            
            // Only process response_item for structured messages
            if msg_type == Some("response_item") {
                if let Some(payload) = data.get("payload") {
                    let item_type = payload.get("type").and_then(|t| t.as_str());
                    let role = payload.get("role").and_then(|r| r.as_str());
                    let timestamp = data.get("timestamp").and_then(|t| t.as_i64());
                    
                    // User messages
                    if role == Some("user") && item_type == Some("message") {
                        if let Some(content_list) = payload.get("content").and_then(|c| c.as_array()) {
                            let text_parts: Vec<String> = content_list.iter()
                                .filter_map(|item| {
                                    if item.get("type").and_then(|t| t.as_str()) == Some("input_text") {
                                        item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if !text_parts.is_empty() {
                                messages.push(SessionMessage {
                                    role: "user".to_string(),
                                    content: text_parts.join("\n\n"),
                                    timestamp,
                                });
                            }
                        }
                    }
                    // Assistant messages
                    else if role == Some("assistant") && item_type == Some("message") {
                        if let Some(content_list) = payload.get("content").and_then(|c| c.as_array()) {
                            let text_parts: Vec<String> = content_list.iter()
                                .filter_map(|item| {
                                    let item_type = item.get("type").and_then(|t| t.as_str());
                                    if item_type == Some("output_text") || item_type == Some("text") {
                                        item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if !text_parts.is_empty() {
                                messages.push(SessionMessage {
                                    role: "assistant".to_string(),
                                    content: text_parts.join("\n\n"),
                                    timestamp,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(messages)
}

// Parse Claude Code messages from JSONL content
fn parse_claude_jsonl(content: &str) -> Result<Vec<SessionMessage>> {
    use std::io::{BufRead, BufReader};
    
    let mut messages = Vec::new();
    let reader = BufReader::new(content.as_bytes());
    
    for line in reader.lines().flatten() {
        if line.trim().is_empty() {
            continue;
        }
        
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&line) {
            let msg_type = data.get("type").and_then(|t| t.as_str());
            
            if msg_type == Some("user") || msg_type == Some("assistant") {
                let role = msg_type.unwrap();
                let timestamp = data.get("timestamp").and_then(|t| t.as_i64());
                
                if let Some(message) = data.get("message") {
                    let content_val = message.get("content");
                    
                    let content = if let Some(arr) = content_val.and_then(|c| c.as_array()) {
                        arr.iter()
                            .filter_map(|item| {
                                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                    item.get("text").and_then(|t| t.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else if let Some(text) = content_val.and_then(|c| c.as_str()) {
                        text.to_string()
                    } else {
                        continue;
                    };
                    
                    if !content.is_empty() && content != "Warmup" {
                        messages.push(SessionMessage {
                            role: role.to_string(),
                            content,
                            timestamp,
                        });
                    }
                }
            }
        }
    }
    
    Ok(messages)
}

// Session commands
#[tauri::command]
pub async fn get_session_projects(
    cli_type: String,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<PaginatedProjects> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);

    let base_dir = get_cli_base_dir(&cli_type);
    let projects_dir = match cli_type.as_str() {
        "codex" => base_dir.join("sessions"),
        "gemini" => base_dir.join("tmp"),
        _ => base_dir.join("projects"),
    };

    // For Codex, we need special handling since sessions are not in project folders
    if cli_type == "codex" {
        return get_codex_projects(projects_dir, page, page_size);
    }

    // For Gemini, check if sessions are in hash directories with chats subfolder
    if cli_type == "gemini" {
        return get_gemini_projects(projects_dir, page, page_size);
    }

    let mut projects = Vec::new();

    if projects_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&projects_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    if name.is_empty() || name.starts_with('.') {
                        continue;
                    }

                    // Count sessions and calculate size
                    let mut session_count = 0i64;
                    let mut total_size = 0i64;
                    let mut last_modified = 0f64;

                    if let Ok(sessions) = std::fs::read_dir(&path) {
                        for session in sessions.flatten() {
                            let session_path = session.path();
                            if session_path.is_file() {
                                session_count += 1;
                                if let Ok(meta) = session_path.metadata() {
                                    total_size += meta.len() as i64;
                                    if let Ok(mtime) = meta.modified() {
                                        let secs = mtime.duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_secs_f64())
                                            .unwrap_or(0.0);
                                        if secs > last_modified {
                                            last_modified = secs;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let display_name = if cli_type == "claude_code" {
                        // Decode path from project name
                        name.replace("-", "/").replace("_", ":")
                    } else {
                        name.clone()
                    };

                    projects.push(ProjectInfo {
                        name: name.clone(),
                        display_name,
                        full_path: path.to_string_lossy().to_string(),
                        session_count,
                        total_size,
                        last_modified,
                    });
                }
            }
        }
    }

    // Sort by last_modified descending
    projects.sort_by(|a, b| b.last_modified.partial_cmp(&a.last_modified).unwrap_or(std::cmp::Ordering::Equal));

    let total = projects.len() as i64;
    let start = ((page - 1) * page_size) as usize;
    let items: Vec<_> = projects.into_iter().skip(start).take(page_size as usize).collect();

    Ok(PaginatedProjects {
        items,
        total,
        page,
        page_size,
    })
}

#[tauri::command]
pub async fn get_project_sessions(
    cli_type: String,
    project_name: String,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<PaginatedSessions> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);

    // Special handling for Codex
    if cli_type == "codex" {
        return get_codex_sessions(&project_name, page, page_size);
    }

    // Special handling for Gemini
    if cli_type == "gemini" {
        return get_gemini_sessions(&project_name, page, page_size);
    }

    // Claude Code default handling
    let base_dir = get_cli_base_dir(&cli_type);
    let project_dir = base_dir.join("projects").join(&project_name);

    let mut sessions = Vec::new();

    if project_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&project_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let session_id = path.file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    if session_id.is_empty() {
                        continue;
                    }

                    let mut size = 0i64;
                    let mut mtime = 0f64;
                    let mut first_message = String::new();

                    if let Ok(meta) = path.metadata() {
                        size = meta.len() as i64;
                        if let Ok(mt) = meta.modified() {
                            mtime = mt.duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs_f64())
                                .unwrap_or(0.0);
                        }
                    }

                    // Try to read first message from JSON
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            // Claude Code format
                            if let Some(messages) = json.get("messages").and_then(|m| m.as_array()) {
                                for msg in messages {
                                    if msg.get("type").and_then(|t| t.as_str()) == Some("human") {
                                        if let Some(content) = msg.get("content") {
                                            if let Some(arr) = content.as_array() {
                                                for item in arr {
                                                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                                        first_message = text.chars().take(200).collect();
                                                        break;
                                                    }
                                                }
                                            } else if let Some(text) = content.as_str() {
                                                first_message = text.chars().take(200).collect();
                                            }
                                        }
                                        break;
                                    }
                                    // Gemini format
                                    if msg.get("type").and_then(|t| t.as_str()) == Some("user") {
                                        if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                                            first_message = text.chars().take(200).collect();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    sessions.push(SessionInfo {
                        session_id,
                        size,
                        mtime,
                        first_message,
                        git_branch: String::new(),
                        summary: String::new(),
                    });
                }
            }
        }
    }

    // Sort by mtime descending
    sessions.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));

    let total = sessions.len() as i64;
    let start = ((page - 1) * page_size) as usize;
    let items: Vec<_> = sessions.into_iter().skip(start).take(page_size as usize).collect();

    Ok(PaginatedSessions {
        items,
        total,
        page,
        page_size,
    })
}

#[tauri::command]
pub async fn get_session_messages(
    cli_type: String,
    project_name: String,
    session_id: String,
) -> Result<Vec<SessionMessage>> {
    // Special handling for Codex JSONL format
    if cli_type == "codex" {
        return get_codex_messages(&session_id);
    }
    
    let base_dir = get_cli_base_dir(&cli_type);
    let session_file = match cli_type.as_str() {
        "gemini" => base_dir.join("tmp").join(&project_name).join("chats").join(format!("{}.json", session_id)),
        _ => base_dir.join("projects").join(&project_name).join(format!("{}.jsonl", session_id)),
    };

    let content = std::fs::read_to_string(&session_file)
        .map_err(|e| format!("Failed to read session file: {}", e))?;

    // For Claude Code JSONL format
    if cli_type == "claude_code" {
        return parse_claude_jsonl(&content);
    }
    
    // For Gemini JSON format
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse session JSON: {}", e))?;

    let mut messages = Vec::new();

    // Try to parse messages in different formats
    if let Some(msgs) = json.get("messages").and_then(|m| m.as_array()) {
        // Standard format with messages array
        for msg in msgs {
            let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let role = match msg_type {
                "human" | "user" => "user",
                "assistant" | "ai" | "gemini" => "assistant",  // Add "gemini" type
                _ => continue,
            };

            let content = if let Some(content_val) = msg.get("content") {
                if let Some(arr) = content_val.as_array() {
                    arr.iter()
                        .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else if let Some(text) = content_val.as_str() {
                    text.to_string()
                } else {
                    continue;
                }
            } else {
                continue;
            };

            let timestamp = msg.get("timestamp").and_then(|t| t.as_str()).map(|s| {
                chrono::DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.timestamp())
            }).flatten();

            messages.push(SessionMessage {
                role: role.to_string(),
                content,
                timestamp,
            });
        }
    } else if let Some(conversation) = json.as_object() {
        // Try to parse as flat object with role-based keys
        for (key, value) in conversation {
            if key == "id" || key == "title" || key == "created_at" || key == "updated_at" {
                continue;
            }
            let role = if key.starts_with("user") || key.starts_with("human") {
                "user"
            } else if key.starts_with("assistant") || key.starts_with("ai") {
                "assistant"
            } else {
                continue;
            };

            if let Some(text) = value.as_str() {
                messages.push(SessionMessage {
                    role: role.to_string(),
                    content: text.to_string(),
                    timestamp: None,
                });
            }
        }
    }

    Ok(messages)
}

#[tauri::command]
pub async fn delete_session(
    cli_type: String,
    project_name: String,
    session_id: String,
) -> Result<()> {
    let base_dir = get_cli_base_dir(&cli_type);
    let session_file = match cli_type.as_str() {
        "codex" => base_dir.join("sessions").join(format!("{}.jsonl", session_id)),
        "gemini" => base_dir.join("tmp").join(&project_name).join("chats").join(format!("{}.json", session_id)),
        _ => base_dir.join("projects").join(&project_name).join(format!("{}.jsonl", session_id)),
    };

    std::fs::remove_file(&session_file)
        .map_err(|e| format!("Failed to delete session: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn delete_project(
    cli_type: String,
    project_name: String,
) -> Result<()> {
    let base_dir = get_cli_base_dir(&cli_type);
    
    if cli_type == "codex" {
        // For Codex, delete all session files matching the project cwd
        use walkdir::WalkDir;
        let sessions_dir = base_dir.join("sessions");
        if sessions_dir.exists() {
            // Use WalkDir to recursively search all subdirectories
            for entry in WalkDir::new(&sessions_dir)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() {
                    let filename = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    if filename.starts_with("rollout-") && filename.ends_with(".jsonl") {
                        if let Some(cwd) = extract_codex_cwd(path) {
                            if cwd == project_name {
                                let _ = std::fs::remove_file(path);
                            }
                        }
                    }
                }
            }
        }
        return Ok(());
    }
    
    // For Claude Code and Gemini, delete the project directory
    let project_dir = match cli_type.as_str() {
        "gemini" => base_dir.join("tmp").join(&project_name),
        _ => base_dir.join("projects").join(&project_name),
    };

    std::fs::remove_dir_all(&project_dir)
        .map_err(|e| format!("Failed to delete project: {}", e))?;

    Ok(())
}

/// 退出应用程序（导入后需要手动重启）
async fn exit_application() -> Result<()> {
    tokio::spawn(async {
        // 延迟 3 秒，等待响应返回前端并给用户时间看提示
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        std::process::exit(0);
    });

    Ok(())
}

// Backup commands
#[tauri::command]
pub async fn get_webdav_settings(db: State<'_, SqlitePool>) -> Result<WebdavSettings> {
    // Try to get existing settings
    let settings = sqlx::query_as::<_, WebdavSettings>(
        "SELECT url, username, password FROM webdav_settings WHERE id = 1"
    )
    .fetch_optional(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    match settings {
        Some(s) => Ok(s),
        None => {
            // Create default settings
            let now = chrono::Utc::now().timestamp();
            sqlx::query(
                "INSERT INTO webdav_settings (id, url, username, password, updated_at) VALUES (1, '', '', '', ?)"
            )
            .bind(now)
            .execute(db.inner())
            .await
            .map_err(|e| e.to_string())?;

            Ok(WebdavSettings {
                url: String::new(),
                username: String::new(),
                password: String::new(),
            })
        }
    }
}

#[tauri::command]
pub async fn update_webdav_settings(
    db: State<'_, SqlitePool>,
    input: WebdavSettingsUpdate,
) -> Result<WebdavSettings> {
    let now = chrono::Utc::now().timestamp();
    let current = get_webdav_settings(db.clone()).await?;

    sqlx::query(
        "UPDATE webdav_settings SET url = ?, username = ?, password = ?, updated_at = ? WHERE id = 1"
    )
    .bind(input.url.unwrap_or(current.url))
    .bind(input.username.unwrap_or(current.username))
    .bind(input.password.unwrap_or(current.password))
    .bind(now)
    .execute(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    get_webdav_settings(db).await
}

#[tauri::command]
pub async fn test_webdav_connection(
    url: String,
    username: String,
    password: String,
) -> Result<bool> {
    use reqwest::Client;

    let client = Client::new();
    let response = client
        .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
        .basic_auth(&username, Some(&password))
        .header("Depth", "0")
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    Ok(response.status().is_success() || response.status().as_u16() == 207)
}

#[tauri::command]
pub async fn export_to_local() -> Result<Vec<u8>> {
    // Get the database path from config
    let db_path = get_data_dir().join("ccg_gateway.db");

    // Read the database file
    let content = std::fs::read(&db_path)
        .map_err(|e| format!("Failed to read database: {}", e))?;

    Ok(content)
}

#[tauri::command]
pub async fn import_from_local(data: Vec<u8>) -> Result<()> {
    let db_path = get_data_dir().join("ccg_gateway.db");

    // Write the database file
    std::fs::write(&db_path, &data)
        .map_err(|e| format!("Failed to write database: {}", e))?;

    // 退出应用，用户需手动重启
    exit_application().await?;

    Ok(())
}

#[tauri::command]
pub async fn export_to_webdav(db: State<'_, SqlitePool>) -> Result<String> {
    use reqwest::Client;

    let settings = get_webdav_settings(db.clone()).await?;
    if settings.url.is_empty() {
        return Err("WebDAV URL not configured".to_string());
    }

    // Read database file
    let db_path = get_data_dir().join("ccg_gateway.db");
    let content = std::fs::read(&db_path)
        .map_err(|e| format!("Failed to read database: {}", e))?;

    // Generate filename
    let filename = format!(
        "ccg_gateway_{}.db",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );

    // Ensure remote directory exists
    let client = Client::new();
    let remote_dir = format!("{}/ccg-gateway-backup", settings.url.trim_end_matches('/'));

    // Try to create directory (ignore error if exists)
    let _ = client
        .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), &remote_dir)
        .basic_auth(&settings.username, Some(&settings.password))
        .send()
        .await;

    // Upload file
    let remote_file = format!("{}/{}", remote_dir, filename);
    let response = client
        .put(&remote_file)
        .basic_auth(&settings.username, Some(&settings.password))
        .body(content)
        .send()
        .await
        .map_err(|e| format!("Upload failed: {}", e))?;

    if !response.status().is_success() && response.status().as_u16() != 201 {
        return Err(format!("Upload failed with status: {}", response.status()));
    }

    Ok(filename)
}

#[tauri::command]
pub async fn list_webdav_backups(db: State<'_, SqlitePool>) -> Result<Vec<WebdavBackup>> {
    use reqwest::Client;

    let settings = get_webdav_settings(db).await?;
    if settings.url.is_empty() {
        return Err("WebDAV URL not configured".to_string());
    }

    let client = Client::new();
    let remote_dir = format!("{}/ccg-gateway-backup", settings.url.trim_end_matches('/'));

    let response = client
        .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &remote_dir)
        .basic_auth(&settings.username, Some(&settings.password))
        .header("Depth", "1")
        .header("Content-Type", "application/xml")
        .body(r#"<?xml version="1.0" encoding="utf-8"?>
            <propfind xmlns="DAV:">
                <prop>
                    <getcontentlength/>
                    <getlastmodified/>
                </prop>
            </propfind>"#)
        .send()
        .await
        .map_err(|e| format!("Failed to list backups: {}", e))?;

    if !response.status().is_success() && response.status().as_u16() != 207 {
        return Ok(Vec::new());
    }

    let body = response.text().await.map_err(|e| e.to_string())?;

    // Parse XML response using quick-xml
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(&body);
    reader.config_mut().trim_text(true);

    let mut backups = Vec::new();
    let mut current_href = String::new();
    let mut current_size: i64 = 0;
    let mut current_modified = String::new();
    let mut in_response = false;
    let mut current_tag = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name.ends_with(":response") || name == "response" {
                    in_response = true;
                    current_href.clear();
                    current_size = 0;
                    current_modified.clear();
                }
                current_tag = name;
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default().trim().to_string();
                if in_response && !text.is_empty() {
                    if current_tag.ends_with(":href") || current_tag == "href" {
                        current_href = text;
                    } else if current_tag.ends_with(":getcontentlength") || current_tag == "getcontentlength" {
                        current_size = text.parse::<i64>().unwrap_or(0);
                    } else if current_tag.ends_with(":getlastmodified") || current_tag == "getlastmodified" {
                        current_modified = text;
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name.ends_with(":response") || name == "response" {
                    in_response = false;
                    
                    // Check if this is a .db file we care about
                    if current_href.contains("ccg_gateway_") && current_href.ends_with(".db") {
                        // Extract filename from href
                        if let Some(start) = current_href.rfind('/') {
                            let filename = current_href[start + 1..].to_string();
                            if filename.starts_with("ccg_gateway_") {
                                backups.push(WebdavBackup {
                                    filename,
                                    size: current_size,
                                    modified: current_modified.clone(),
                                });
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error at position {}: {}", reader.buffer_position(), e)),
            _ => {}
        }
        buf.clear();
    }

    // Sort by filename descending (newest first based on timestamp in name)
    backups.sort_by(|a, b| b.filename.cmp(&a.filename));

    Ok(backups)
}

#[tauri::command]
pub async fn import_from_webdav(
    db: State<'_, SqlitePool>,
    filename: String,
) -> Result<()> {
    use reqwest::Client;

    let settings = get_webdav_settings(db).await?;
    if settings.url.is_empty() {
        return Err("WebDAV URL not configured".to_string());
    }

    let client = Client::new();
    let remote_file = format!(
        "{}/ccg-gateway-backup/{}",
        settings.url.trim_end_matches('/'),
        filename
    );

    let response = client
        .get(&remote_file)
        .basic_auth(&settings.username, Some(&settings.password))
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
    }

    let content = response.bytes().await.map_err(|e| e.to_string())?;

    // Write to database file
    let db_path = get_data_dir().join("ccg_gateway.db");

    std::fs::write(&db_path, &content)
        .map_err(|e| format!("Failed to write database: {}", e))?;

    // 退出应用，用户需手动重启
    exit_application().await?;

    Ok(())
}

#[tauri::command]
pub async fn delete_webdav_backup(
    db: State<'_, SqlitePool>,
    filename: String,
) -> Result<()> {
    use reqwest::Client;

    let settings = get_webdav_settings(db).await?;
    if settings.url.is_empty() {
        return Err("WebDAV URL not configured".to_string());
    }

    let client = Client::new();
    let remote_file = format!(
        "{}/ccg-gateway-backup/{}",
        settings.url.trim_end_matches('/'),
        filename
    );

    let response = client
        .delete(&remote_file)
        .basic_auth(&settings.username, Some(&settings.password))
        .send()
        .await
        .map_err(|e| format!("Delete failed: {}", e))?;

    if !response.status().is_success() && response.status().as_u16() != 204 {
        return Err(format!("Delete failed with status: {}", response.status()));
    }

    Ok(())
}
