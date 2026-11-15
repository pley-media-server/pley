-- AirPlay devices the server paired with (src/airplay): by the id the
-- device announces (its `deviceid`, lowercase hex without colons), the
-- pairing id and Ed25519 public key it gave when paired, its name then,
-- and who typed the PIN. The server's own key pair is derived from
-- secret.key, so nothing private is kept here. Times are unix seconds.
CREATE TABLE airplay_pairings (
    device_id   TEXT    PRIMARY KEY,
    pairing_id  TEXT    NOT NULL,
    public_key  BLOB    NOT NULL,
    name        TEXT    NOT NULL,
    paired_by   INTEGER REFERENCES users(id) ON DELETE SET NULL,
    paired_at   INTEGER NOT NULL
);
CREATE INDEX airplay_pairings_user ON airplay_pairings(paired_by) WHERE paired_by IS NOT NULL;
