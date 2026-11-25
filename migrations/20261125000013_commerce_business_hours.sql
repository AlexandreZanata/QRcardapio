CREATE TABLE IF NOT EXISTS commercial_settings (
    establishment_id TEXT PRIMARY KEY NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    time_zone TEXT NOT NULL DEFAULT 'America/Sao_Paulo',
    delivery INTEGER NOT NULL DEFAULT 0 CHECK (delivery IN (0, 1)),
    pickup INTEGER NOT NULL DEFAULT 0 CHECK (pickup IN (0, 1)),
    minimum_centavos INTEGER NOT NULL DEFAULT 0 CHECK (minimum_centavos BETWEEN 0 AND 1000000),
    acceptance_minutes INTEGER NOT NULL DEFAULT 10 CHECK (acceptance_minutes BETWEEN 3 AND 30),
    paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1)),
    paused_until INTEGER CHECK (paused_until IS NULL OR (paused = 1 AND paused_until > 0))
) STRICT;

CREATE TABLE IF NOT EXISTS business_hours (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    weekday INTEGER NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    windows_json TEXT NOT NULL CHECK (json_valid(windows_json) AND json_type(windows_json) = 'array'),
    PRIMARY KEY (establishment_id, weekday)
) STRICT;

CREATE TABLE IF NOT EXISTS business_hour_exceptions (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    local_date TEXT NOT NULL CHECK (length(local_date) = 10),
    windows_json TEXT NOT NULL CHECK (json_valid(windows_json) AND json_type(windows_json) = 'array'),
    PRIMARY KEY (establishment_id, local_date)
) STRICT;
