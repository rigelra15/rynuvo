-- Bytes moved through the voice relay, one row per finished voice session.
CREATE TABLE voice_usage (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    channel_id INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER NOT NULL,
    bytes_in INTEGER NOT NULL,
    bytes_out INTEGER NOT NULL
);
CREATE INDEX voice_usage_ended ON voice_usage(ended_at);
