-- Create ssh_keys table for standalone local SSH key management.
-- Keys live in `~/.ssh/deploykeys/<directory>/` with an isolated directory per
-- key. `directory` is the on-disk folder name (unique); `remark` is a free-form
-- user note. `comment` is the identity embedded in the public key line
-- (conventionally an email) and is immutable after creation.
CREATE TABLE ssh_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    directory TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    public_key TEXT NOT NULL,
    public_key_fingerprint TEXT NOT NULL,
    private_key_path TEXT NOT NULL,
    public_key_path TEXT NOT NULL,
    comment TEXT NOT NULL DEFAULT '',
    remark TEXT NOT NULL DEFAULT '',
    target_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (target_id) REFERENCES targets(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_ssh_keys_directory ON ssh_keys(directory);
CREATE INDEX idx_ssh_keys_target_id ON ssh_keys(target_id);
