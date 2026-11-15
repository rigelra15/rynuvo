-- "Rynuvo Music": the author of music replies in chat. No devices, so it can never sign in.
INSERT INTO users(nickname, nickname_key, is_admin, avatar_icon, avatar_color)
VALUES ('Rynuvo Music', '#bot:music', 0, 'music', 'violet')
ON CONFLICT(nickname_key) DO NOTHING;
