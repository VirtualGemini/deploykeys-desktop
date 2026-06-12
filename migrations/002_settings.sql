-- Application-wide key/value settings (e.g. UI language preference).
CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
