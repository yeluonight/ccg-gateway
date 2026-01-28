use sqlx::SqlitePool;

/// Record a successful request for a provider
/// Resets consecutive_failures to 0
/// Returns (had_previous_failures) to indicate if the provider was recovering
pub async fn record_success(db: &SqlitePool, provider_id: i64) -> Result<bool, sqlx::Error> {
    let now = chrono::Utc::now().timestamp();

    // Check if provider had previous failures
    let had_failures: Option<(i64,)> = sqlx::query_as(
        "SELECT consecutive_failures FROM providers WHERE id = ?",
    )
    .bind(provider_id)
    .fetch_optional(db)
    .await?;

    let had_previous_failures = had_failures.map(|(cf,)| cf > 0).unwrap_or(false);

    sqlx::query(
        r#"
        UPDATE providers
        SET consecutive_failures = 0,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(provider_id)
    .execute(db)
    .await?;

    Ok(had_previous_failures)
}

/// Record a failed request for a provider
/// Increments consecutive_failures and blacklists if threshold is reached
/// Returns (was_blacklisted, provider_name) tuple
pub async fn record_failure(db: &SqlitePool, provider_id: i64) -> Result<(bool, String), sqlx::Error> {
    let now = chrono::Utc::now().timestamp();

    // Get current provider state including name
    let provider: Option<(i64, i64, i64, String)> = sqlx::query_as(
        "SELECT consecutive_failures, failure_threshold, blacklist_minutes, name FROM providers WHERE id = ?",
    )
    .bind(provider_id)
    .fetch_optional(db)
    .await?;

    let Some((consecutive_failures, failure_threshold, blacklist_minutes, provider_name)) = provider else {
        return Ok((false, String::new()));
    };

    let new_failures = consecutive_failures + 1;

    // Check if we should blacklist
    let was_blacklisted = if new_failures >= failure_threshold {
        let blacklist_until = now + (blacklist_minutes * 60);
        sqlx::query(
            r#"
            UPDATE providers
            SET consecutive_failures = ?,
                blacklisted_until = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(new_failures)
        .bind(blacklist_until)
        .bind(now)
        .bind(provider_id)
        .execute(db)
        .await?;

        tracing::warn!(
            provider_id = provider_id,
            failures = new_failures,
            blacklist_until = blacklist_until,
            "Provider blacklisted due to consecutive failures"
        );
        true
    } else {
        sqlx::query(
            r#"
            UPDATE providers
            SET consecutive_failures = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(new_failures)
        .bind(now)
        .bind(provider_id)
        .execute(db)
        .await?;
        false
    };

    Ok((was_blacklisted, provider_name))
}

/// Reset provider failures and remove blacklist
pub async fn reset_failures(db: &SqlitePool, provider_id: i64) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        r#"
        UPDATE providers
        SET consecutive_failures = 0,
            blacklisted_until = NULL,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(provider_id)
    .execute(db)
    .await?;

    Ok(())
}
