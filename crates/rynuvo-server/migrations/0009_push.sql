-- One push registration per signed-in device (an Android phone with Firebase Cloud Messaging).
-- `key_hash`: the hash of the key the phone uses to fetch its notifications, and nothing else.
CREATE TABLE push_tokens (
    device_id INTEGER PRIMARY KEY REFERENCES devices(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token TEXT NOT NULL,
    key_hash BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX push_tokens_user ON push_tokens(user_id);
