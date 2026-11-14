-- Screen-share bytes that went through the relay, one row per finished streamer–viewer pair.
CREATE TABLE stream_usage (
    id INTEGER PRIMARY KEY,
    streamer_id INTEGER NOT NULL,
    viewer_id INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER NOT NULL,
    relay_bytes INTEGER NOT NULL
);
CREATE INDEX stream_usage_ended ON stream_usage(ended_at);
