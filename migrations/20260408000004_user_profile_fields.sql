ALTER TABLE users
    ADD COLUMN avatar TEXT NULL AFTER display_name,
    ADD COLUMN bio TEXT NULL AFTER avatar,
    ADD COLUMN timezone VARCHAR(50) NOT NULL DEFAULT 'UTC' AFTER bio,
    ADD COLUMN locale VARCHAR(10) NOT NULL DEFAULT 'en' AFTER timezone;

UPDATE users
SET timezone = 'UTC'
WHERE timezone IS NULL OR timezone = '';

UPDATE users
SET locale = 'en'
WHERE locale IS NULL OR locale = '';
