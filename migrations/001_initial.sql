-- Create accounts table
CREATE TABLE accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    github_user_id INTEGER NOT NULL UNIQUE,
    login TEXT NOT NULL,
    avatar_url TEXT,
    auth_type TEXT NOT NULL,
    token_ref TEXT NOT NULL,
    refresh_token_ref TEXT,
    token_expires_at INTEGER,
    created_at INTEGER NOT NULL,
    last_login_at INTEGER NOT NULL
);

-- Create repositories table
CREATE TABLE repositories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    github_repo_id INTEGER NOT NULL UNIQUE,
    account_id INTEGER NOT NULL,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    full_name TEXT NOT NULL,
    private INTEGER NOT NULL,
    archived INTEGER NOT NULL,
    default_branch TEXT,
    ssh_url TEXT NOT NULL,
    html_url TEXT NOT NULL,
    language TEXT,
    permissions_snapshot TEXT,
    last_synced_at INTEGER,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

-- Create targets table
CREATE TABLE targets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_type TEXT NOT NULL,
    alias TEXT NOT NULL UNIQUE,
    os TEXT NOT NULL,
    host TEXT,
    port INTEGER,
    username TEXT,
    auth_method TEXT,
    auth_ref TEXT,
    key_base_dir TEXT NOT NULL,
    status TEXT NOT NULL,
    host_key_fingerprint TEXT,
    created_at INTEGER NOT NULL,
    last_checked_at INTEGER
);

-- Application-wide key/value settings (e.g. UI language preference).
CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Create key_bindings table
CREATE TABLE key_bindings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL,
    target_id INTEGER NOT NULL,
    github_deploy_key_id INTEGER,
    deploy_key_title TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    permission TEXT NOT NULL,
    public_key TEXT NOT NULL,
    public_key_fingerprint TEXT NOT NULL,
    private_key_path TEXT NOT NULL,
    private_key_residency TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_verified_at INTEGER,
    FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
    FOREIGN KEY (target_id) REFERENCES targets(id) ON DELETE CASCADE,
    UNIQUE(repo_id, target_id)
);

-- Create indexes
CREATE INDEX idx_key_bindings_status ON key_bindings(status);
CREATE INDEX idx_key_bindings_repo_id ON key_bindings(repo_id);
CREATE INDEX idx_key_bindings_target_id ON key_bindings(target_id);
CREATE INDEX idx_repositories_full_name ON repositories(full_name);
CREATE INDEX idx_repositories_account_id ON repositories(account_id);

-- Create ssh_keys table for standalone SSH key management.
-- Keys live under each target's configured key base directory, with an
-- isolated directory per key. `directory` is unique within a target; `remark`
-- is a free-form user note. `comment` is the identity embedded in the public
-- key line (conventionally an email) and is immutable after creation.
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

CREATE UNIQUE INDEX idx_ssh_keys_target_directory ON ssh_keys(target_id, directory);
CREATE INDEX idx_ssh_keys_target_id ON ssh_keys(target_id);
