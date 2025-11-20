use anyhow::{Context, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn init_pool(database_url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .context("Failed to connect to Postgres Database")
}

pub struct ClosestMatch {
    pub message_id: i32,
    pub distance: u8,
}

/// Returns the closest match to the hash, but not the excluded message id if given.
pub async fn find_closest_match(
    pool: &PgPool,
    chat_id: i64,
    hash: i64,
    threshold: u8,
    exclude_message_id: Option<i32>,
) -> sqlx::Result<Option<ClosestMatch>> {
    let record = sqlx::query!(
        r#"
        SELECT
            message_id,
            bit_count( (phash # $1)::bit(64) ) as distance
        FROM images
        WHERE chat_id = $2
            AND bit_count( (phash # $1)::bit(64) ) <= $3
            AND ($4::INT IS NULL OR message_id != $4)
        ORDER BY distance ASC, message_id ASC
        LIMIT 1
        "#,
        hash,
        chat_id,
        threshold as i32,
        exclude_message_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(record.map(|r| ClosestMatch {
        message_id: r.message_id,
        distance: r.distance.unwrap() as u8,
    }))
}

pub async fn save_image(
    pool: &PgPool,
    chat_id: i64,
    chat_title: &str,
    message_id: i32,
    phash: i64,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        -- First, ensure the chat exists or update its title
        WITH ensure_chat AS (
            INSERT INTO chats (id, title)
            VALUES ($1, $2)
            ON CONFLICT (id) DO UPDATE
            SET title = EXCLUDED.title
        )
        -- Then, insert the image record
        INSERT INTO images (chat_id, message_id, phash)
        VALUES ($1, $3, $4)
        "#,
        chat_id,
        chat_title,
        message_id,
        phash
    )
    .execute(pool)
    .await?;

    Ok(())
}
