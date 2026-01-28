use sqlx::SqlitePool;

use crate::db::models::{Provider, ProviderModelMap};

/// Provider with its model mappings
#[derive(Debug, Clone)]
pub struct ProviderWithMaps {
    pub provider: Provider,
    pub model_maps: Vec<ProviderModelMap>,
}

/// Select an available provider for the given CLI type
/// Returns None if all providers are blacklisted or none are configured
pub async fn select_provider(
    db: &SqlitePool,
    cli_type: &str,
) -> Result<Option<ProviderWithMaps>, sqlx::Error> {
    let now = chrono::Utc::now().timestamp();

    // Query enabled providers ordered by sort_order, excluding blacklisted ones
    let providers = sqlx::query_as::<_, Provider>(
        r#"
        SELECT * FROM providers
        WHERE cli_type = ?
          AND enabled = 1
          AND (blacklisted_until IS NULL OR blacklisted_until <= ?)
        ORDER BY sort_order, id
        "#,
    )
    .bind(cli_type)
    .bind(now)
    .fetch_all(db)
    .await?;

    // Return the first available provider with its model maps
    if let Some(provider) = providers.into_iter().next() {
        let model_maps = sqlx::query_as::<_, ProviderModelMap>(
            "SELECT * FROM provider_model_map WHERE provider_id = ? AND enabled = 1",
        )
        .bind(provider.id)
        .fetch_all(db)
        .await?;

        Ok(Some(ProviderWithMaps { provider, model_maps }))
    } else {
        Ok(None)
    }
}

/// Get all available providers for a CLI type (for fallback scenarios)
pub async fn get_available_providers(
    db: &SqlitePool,
    cli_type: &str,
) -> Result<Vec<ProviderWithMaps>, sqlx::Error> {
    let now = chrono::Utc::now().timestamp();

    let providers = sqlx::query_as::<_, Provider>(
        r#"
        SELECT * FROM providers
        WHERE cli_type = ?
          AND enabled = 1
          AND (blacklisted_until IS NULL OR blacklisted_until <= ?)
        ORDER BY sort_order, id
        "#,
    )
    .bind(cli_type)
    .bind(now)
    .fetch_all(db)
    .await?;

    let mut result = Vec::new();
    for provider in providers {
        let model_maps = sqlx::query_as::<_, ProviderModelMap>(
            "SELECT * FROM provider_model_map WHERE provider_id = ? AND enabled = 1",
        )
        .bind(provider.id)
        .fetch_all(db)
        .await?;

        result.push(ProviderWithMaps { provider, model_maps });
    }

    Ok(result)
}
