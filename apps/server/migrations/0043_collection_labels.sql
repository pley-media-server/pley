-- Labels on collections, as on items (0015_labels.sql): Kometa marks the
-- collections it makes with them and finds them again by label, through
-- the Plex-compatible API.
CREATE TABLE collection_labels (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    label         TEXT    NOT NULL COLLATE NOCASE,
    PRIMARY KEY (collection_id, label)
) WITHOUT ROWID;
CREATE INDEX collection_labels_label ON collection_labels(label, collection_id);
