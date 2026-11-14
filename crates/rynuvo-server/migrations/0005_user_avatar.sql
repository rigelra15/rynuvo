-- Avatar picked by the user: keys from rynuvo_proto::AVATAR_ICONS / AVATAR_COLORS (NULL = default).
ALTER TABLE users ADD COLUMN avatar_icon TEXT;
ALTER TABLE users ADD COLUMN avatar_color TEXT;
