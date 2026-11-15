pub fn ddl(schema: &str) -> Vec<String> {
    vec![
        format!("CREATE SCHEMA IF NOT EXISTS {schema}"),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.app_user (
                user_id uuid PRIMARY KEY,
                login_id text NOT NULL UNIQUE,
                password_hash text NOT NULL,
                disabled boolean NOT NULL DEFAULT false,
                failed_count integer NOT NULL DEFAULT 0,
                locked_until timestamptz,
                created_at timestamptz NOT NULL DEFAULT now()
            )"
        ),
        format!(
            "ALTER TABLE {schema}.app_user ADD COLUMN IF NOT EXISTS status text NOT NULL DEFAULT 'active'
             CHECK (status IN ('pending', 'active', 'disabled'))"
        ),
        format!("ALTER TABLE {schema}.app_user ADD COLUMN IF NOT EXISTS display_name text NOT NULL DEFAULT ''"),
        format!("ALTER TABLE {schema}.app_user ADD COLUMN IF NOT EXISTS is_master boolean NOT NULL DEFAULT false"),
        format!("ALTER TABLE {schema}.app_user ADD COLUMN IF NOT EXISTS requested_at timestamptz NOT NULL DEFAULT now()"),
        format!("UPDATE {schema}.app_user SET status = 'disabled' WHERE disabled AND status <> 'disabled'"),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.login_session (
                token_hash char(64) PRIMARY KEY,
                user_id uuid NOT NULL REFERENCES {schema}.app_user ON DELETE CASCADE,
                csrf_token_hash char(64) NOT NULL,
                created_at timestamptz NOT NULL DEFAULT now(),
                last_seen_at timestamptz NOT NULL DEFAULT now(),
                expires_at timestamptz NOT NULL
            )"
        ),
        format!("CREATE INDEX IF NOT EXISTS login_session_user ON {schema}.login_session (user_id)"),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.thread_owner (
                thread_id uuid PRIMARY KEY,
                user_id uuid NOT NULL REFERENCES {schema}.app_user ON DELETE CASCADE,
                title text,
                created_at timestamptz NOT NULL DEFAULT now(),
                updated_at timestamptz NOT NULL DEFAULT now()
            )"
        ),
        format!("CREATE INDEX IF NOT EXISTS thread_owner_user ON {schema}.thread_owner (user_id, updated_at DESC)"),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.usage (
                user_id uuid NOT NULL REFERENCES {schema}.app_user ON DELETE CASCADE,
                day date NOT NULL,
                tokens bigint NOT NULL DEFAULT 0,
                PRIMARY KEY (user_id, day)
            )"
        ),
    ]
}

pub fn valid_schema_name(schema: &str) -> bool {
    !schema.is_empty()
        && schema.len() <= 48
        && schema.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && schema.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}
