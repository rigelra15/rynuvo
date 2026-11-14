use rynuvo_proto::{Channel, ChannelKind};
use sqlx::SqlitePool;

use crate::error::AppResult;

type ChannelRow = (i64, String, String, Option<String>, i64, i64);

pub(crate) fn row_to_channel((id, kind, name, topic, owner_id, position): ChannelRow) -> Channel {
    Channel { id, kind: ChannelKind::parse(&kind).unwrap_or(ChannelKind::Text), name, topic, owner_id, position }
}

pub async fn load_channel(db: &SqlitePool, id: i64) -> AppResult<Option<Channel>> {
    let row: Option<ChannelRow> = sqlx::query_as("SELECT id, kind, name, topic, owner_id, position FROM channels WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(row.map(row_to_channel))
}
