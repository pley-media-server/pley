-- Exceptions to what a profile may see, by label (see catalog::VISIBLE):
-- titles with a 'show' label show even above its age limit, and unrated;
-- titles with a 'hide' label never show, whatever their rating. A label
-- counts on the title or on what it's part of (a show, an album), and is
-- matched without regard to case, as item_labels keeps it. 'hide' wins.
CREATE TABLE user_label_rules (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label   TEXT    NOT NULL COLLATE NOCASE,
    kind    TEXT    NOT NULL CHECK (kind IN ('show', 'hide')),
    PRIMARY KEY (user_id, label)
) WITHOUT ROWID;

-- An invitation's rules, given to the account it makes: JSON arrays of
-- labels, as its library_ids.
ALTER TABLE invites ADD COLUMN show_labels TEXT NOT NULL DEFAULT '[]';
ALTER TABLE invites ADD COLUMN hide_labels TEXT NOT NULL DEFAULT '[]';

-- What a listing shows now changes with an item's labels and a profile's
-- rules: their listing ETags change with them (see 0013_query_indexes.sql).
CREATE TRIGGER listing_labels_insert AFTER INSERT ON item_labels BEGIN
    INSERT INTO listing_versions (scope, id, version)
    SELECT 'library', library_id, 1 FROM items WHERE id = NEW.item_id
    ON CONFLICT (scope, id) DO UPDATE SET version = version + 1;
END;
CREATE TRIGGER listing_labels_delete AFTER DELETE ON item_labels BEGIN
    INSERT INTO listing_versions (scope, id, version)
    SELECT 'library', library_id, 1 FROM items WHERE id = OLD.item_id
    ON CONFLICT (scope, id) DO UPDATE SET version = version + 1;
END;
CREATE TRIGGER listing_rules_insert AFTER INSERT ON user_label_rules BEGIN
    INSERT INTO listing_versions (scope, id, version) VALUES ('user', NEW.user_id, 1)
    ON CONFLICT (scope, id) DO UPDATE SET version = version + 1;
END;
CREATE TRIGGER listing_rules_delete AFTER DELETE ON user_label_rules BEGIN
    INSERT INTO listing_versions (scope, id, version) VALUES ('user', OLD.user_id, 1)
    ON CONFLICT (scope, id) DO UPDATE SET version = version + 1;
END;
