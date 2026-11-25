CREATE TABLE IF NOT EXISTS menu_draft_revisions (
    establishment_id TEXT PRIMARY KEY NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK (typeof(revision) = 'integer' AND revision > 0)
);

CREATE TRIGGER IF NOT EXISTS track_menu_draft_edits
AFTER UPDATE OF has_unpublished_changes ON establishments
WHEN NEW.has_unpublished_changes = 1
BEGIN
    INSERT INTO menu_draft_revisions (establishment_id, revision) VALUES (NEW.id, 1)
    ON CONFLICT (establishment_id) DO UPDATE SET revision = revision + 1;
END;
