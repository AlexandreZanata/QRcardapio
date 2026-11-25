-- Keep the operator and event identifiers even if an account or the event is removed.
CREATE TABLE IF NOT EXISTS event_consumer_replays (
    id INTEGER PRIMARY KEY,
    consumer TEXT NOT NULL,
    event_seq INTEGER NOT NULL,
    actor_account_id TEXT NOT NULL,
    requested_at INTEGER NOT NULL,
    previous_error_key TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_event_consumer_replays_generation
    ON event_consumer_replays (consumer, event_seq, id);

CREATE TRIGGER IF NOT EXISTS event_consumer_replays_append_only_update
BEFORE UPDATE ON event_consumer_replays
BEGIN SELECT RAISE(ABORT, 'event consumer replays are append-only'); END;

CREATE TRIGGER IF NOT EXISTS event_consumer_replays_append_only_delete
BEFORE DELETE ON event_consumer_replays
BEGIN SELECT RAISE(ABORT, 'event consumer replays are append-only'); END;
