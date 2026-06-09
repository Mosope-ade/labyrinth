use sqlx::PgPool;
use uuid::Uuid;

use super::schema::UserRow;

pub async fn find_by_email(pool: &PgPool, email: &str) -> sqlx::Result<Option<UserRow>> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, tier, created_at, updated_at
         FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<UserRow>> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, tier, created_at, updated_at
         FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn create(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
    tier: &str,
) -> sqlx::Result<UserRow> {
    sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (id, email, password_hash, tier, created_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), NOW())
         RETURNING id, email, password_hash, tier, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(email)
    .bind(password_hash)
    .bind(tier)
    .fetch_one(pool)
    .await
}

pub async fn update_tier(pool: &PgPool, id: Uuid, tier: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET tier = $1, updated_at = NOW() WHERE id = $2")
        .bind(tier)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
