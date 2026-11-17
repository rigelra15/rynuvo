-- A profile picture: an id from images (NULL = the icon or letter).
ALTER TABLE users ADD COLUMN avatar_image TEXT;
