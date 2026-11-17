-- How far each user has read each text channel (for unread dots, shared across their devices).
CREATE TABLE read_state (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    last_read INTEGER NOT NULL,
    PRIMARY KEY (user_id, channel_id)
) WITHOUT ROWID;

-- Start everyone at "read up to now", so the first launch after this isn't a wall of dots.
INSERT INTO read_state (user_id, channel_id, last_read)
SELECT u.id, c.id, (SELECT MAX(m.id) FROM messages m WHERE m.channel_id = c.id)
FROM users u, channels c
WHERE c.kind = 'text' AND EXISTS (SELECT 1 FROM messages m WHERE m.channel_id = c.id);
