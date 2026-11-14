CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    nickname TEXT NOT NULL,
    nickname_key TEXT NOT NULL UNIQUE,
    is_admin INTEGER NOT NULL DEFAULT 0,
    removed_at INTEGER
);
CREATE TABLE devices (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    token_hash BLOB NOT NULL UNIQUE,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen INTEGER
);
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE transfer_codes (
    code_hash BLOB PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    expires_at INTEGER NOT NULL,
    used INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE channels (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('text', 'voice')),
    name TEXT NOT NULL,
    topic TEXT,
    owner_id INTEGER NOT NULL REFERENCES users(id),
    position INTEGER NOT NULL
);
CREATE TABLE images (
    id TEXT PRIMARY KEY,
    mime TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    bytes INTEGER NOT NULL,
    uploader_id INTEGER NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL
);
CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_id INTEGER NOT NULL REFERENCES users(id),
    content TEXT NOT NULL,
    image_id TEXT REFERENCES images(id),
    created_at INTEGER NOT NULL,
    edited_at INTEGER
);
CREATE INDEX messages_channel_id ON messages(channel_id, id);
CREATE TABLE diag_reports (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    kind TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
