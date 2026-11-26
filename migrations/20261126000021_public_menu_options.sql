CREATE TABLE IF NOT EXISTS public_menu_option_preferences (
    establishment_id TEXT PRIMARY KEY NOT NULL REFERENCES establishments(id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
