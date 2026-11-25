-- Dispatcher metadata extends the outbox without rewriting its event rows.
CREATE TABLE IF NOT EXISTS event_consumer_checkpoints (
    consumer TEXT PRIMARY KEY NOT NULL,
    last_seq INTEGER NOT NULL DEFAULT 0 CHECK (last_seq >= 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS event_consumer_receipts (
    consumer TEXT NOT NULL REFERENCES event_consumer_checkpoints(consumer) ON DELETE CASCADE,
    event_id TEXT NOT NULL REFERENCES domain_events(event_id) ON DELETE CASCADE,
    processed_at INTEGER NOT NULL,
    PRIMARY KEY (consumer, event_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_event_consumer_receipts_event
    ON event_consumer_receipts (event_id);

CREATE TABLE IF NOT EXISTS event_consumer_failures (
    consumer TEXT NOT NULL REFERENCES event_consumer_checkpoints(consumer) ON DELETE CASCADE,
    event_seq INTEGER NOT NULL REFERENCES domain_events(seq) ON DELETE CASCADE,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL,
    next_attempt_at INTEGER,
    error_key TEXT,
    CHECK (
        (status = 'parked' AND attempts = 0 AND next_attempt_at IS NULL AND error_key IS NULL)
        OR (status = 'retry' AND attempts BETWEEN 1 AND 9 AND next_attempt_at IS NOT NULL AND error_key IS NOT NULL)
        OR (status = 'dead' AND attempts = 10 AND next_attempt_at IS NULL AND error_key IS NOT NULL)
    ),
    PRIMARY KEY (consumer, event_seq)
) STRICT, WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_event_consumer_failures_retry
    ON event_consumer_failures (consumer, status, next_attempt_at);
CREATE INDEX IF NOT EXISTS idx_event_consumer_failures_event
    ON event_consumer_failures (event_seq);

CREATE TABLE IF NOT EXISTS event_dispatch_publications (
    event_seq INTEGER PRIMARY KEY REFERENCES domain_events(seq) ON DELETE CASCADE,
    published_at INTEGER
) STRICT;

CREATE INDEX IF NOT EXISTS idx_event_dispatch_pending
    ON event_dispatch_publications (event_seq) WHERE published_at IS NULL;

INSERT INTO event_dispatch_publications (event_seq)
SELECT seq FROM domain_events WHERE true ON CONFLICT (event_seq) DO NOTHING;

CREATE TRIGGER IF NOT EXISTS track_event_publication AFTER INSERT ON domain_events
BEGIN
    INSERT INTO event_dispatch_publications (event_seq) VALUES (NEW.seq);
END;
