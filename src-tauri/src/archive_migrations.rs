//! 归档数据库的版本化迁移。
//!
//! 早期版本没有记录 schema 版本，因此只在 `user_version = 0` 时执行一次
//! 兼容性补全；之后的变更统一追加到 `MIGRATIONS`，不再在业务代码里探测字段。

use rusqlite::Connection;

pub const BASE_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    room_id         INTEGER NOT NULL,
    room_title      TEXT NOT NULL DEFAULT '',
    streamer_uid    INTEGER NOT NULL DEFAULT 0,
    start_time      INTEGER NOT NULL,
    end_time        INTEGER,
    total_revenue   INTEGER NOT NULL DEFAULT 0,
    gift_revenue    INTEGER NOT NULL DEFAULT 0,
    sc_revenue      INTEGER NOT NULL DEFAULT 0,
    guard_revenue   INTEGER NOT NULL DEFAULT 0,
    danmaku_count   INTEGER NOT NULL DEFAULT 0,
    gift_count      INTEGER NOT NULL DEFAULT 0,
    sc_count        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS danmaku (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL REFERENCES sessions(id),
    original_id     TEXT NOT NULL,
    content         TEXT NOT NULL,
    user_uid        INTEGER NOT NULL,
    user_name       TEXT NOT NULL,
    timestamp       INTEGER NOT NULL,
    is_emoticon     INTEGER NOT NULL DEFAULT 0,
    emoticon_url    TEXT
);

CREATE TABLE IF NOT EXISTS gifts (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL REFERENCES sessions(id),
    original_id     TEXT NOT NULL,
    gift_id         INTEGER NOT NULL,
    gift_name       TEXT NOT NULL,
    gift_icon       TEXT,
    num             INTEGER NOT NULL,
    total_value     INTEGER NOT NULL,
    revenue_value   INTEGER,
    blind_gift_id   INTEGER,
    blind_gift_name TEXT,
    blind_gift_total_value INTEGER,
    is_paid         INTEGER NOT NULL DEFAULT 0,
    user_uid        INTEGER NOT NULL,
    user_name       TEXT NOT NULL,
    timestamp       INTEGER NOT NULL,
    guard_level     INTEGER,
    batch_combo_id  TEXT,
    combo_total_coin INTEGER,
    super_batch_gift_num INTEGER,
    combo_resources_id INTEGER,
    combo_stay_time INTEGER,
    show_batch_combo_send INTEGER
);

CREATE TABLE IF NOT EXISTS super_chats (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL REFERENCES sessions(id),
    original_id     TEXT NOT NULL,
    content         TEXT NOT NULL,
    price           INTEGER NOT NULL,
    user_uid        INTEGER NOT NULL,
    user_name       TEXT NOT NULL,
    background_color TEXT,
    duration        INTEGER NOT NULL,
    start_time      INTEGER NOT NULL
);
"#;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "archive_query_indexes",
        sql: r#"
CREATE INDEX IF NOT EXISTS idx_sessions_start ON sessions(start_time DESC);
CREATE INDEX IF NOT EXISTS idx_sessions_room_start ON sessions(room_id, start_time DESC);
CREATE INDEX IF NOT EXISTS idx_danmaku_session_time ON danmaku(session_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_danmaku_user_latest ON danmaku(user_uid, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_gifts_session_time ON gifts(session_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_gifts_user_latest ON gifts(user_uid, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_gifts_original_id ON gifts(session_id, original_id);
CREATE INDEX IF NOT EXISTS idx_sc_session_time ON super_chats(session_id, start_time DESC);
CREATE INDEX IF NOT EXISTS idx_sc_user_latest ON super_chats(user_uid, start_time DESC);
"#,
    },
    Migration {
        version: 2,
        name: "unified_archive_event_view",
        sql: r#"
CREATE VIEW IF NOT EXISTS archive_events AS
SELECT
    'danmaku' AS event_type,
    d.id,
    d.session_id,
    s.room_id,
    s.room_title,
    d.content,
    NULL AS detail,
    d.user_uid,
    d.user_name,
    d.timestamp,
    NULL AS amount,
    NULL AS quantity,
    d.emoticon_url AS image_url,
    d.is_emoticon,
    0 AS is_paid,
    NULL AS guard_level
FROM danmaku d
JOIN sessions s ON s.id = d.session_id
UNION ALL
SELECT
    'gift',
    g.id,
    g.session_id,
    s.room_id,
    s.room_title,
    g.gift_name,
    g.blind_gift_name,
    g.user_uid,
    g.user_name,
    g.timestamp,
    g.total_value,
    g.num,
    g.gift_icon,
    0,
    g.is_paid,
    g.guard_level
FROM gifts g
JOIN sessions s ON s.id = g.session_id
UNION ALL
SELECT
    'superchat',
    sc.id,
    sc.session_id,
    s.room_id,
    s.room_title,
    sc.content,
    NULL,
    sc.user_uid,
    sc.user_name,
    sc.start_time,
    sc.price,
    1,
    NULL,
    0,
    1,
    NULL
FROM super_chats sc
JOIN sessions s ON s.id = sc.session_id;
"#,
    },
    Migration {
        version: 3,
        name: "global_event_time_indexes",
        sql: r#"
CREATE INDEX IF NOT EXISTS idx_danmaku_time ON danmaku(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_gifts_time ON gifts(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_sc_time ON super_chats(start_time DESC);
"#,
    },
];

pub fn initialize(conn: &mut Connection) -> Result<(), String> {
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|e| format!("启用归档外键失败: {e}"))?;
    conn.execute_batch(BASE_SCHEMA_SQL)
        .map_err(|e| format!("初始化归档表失败: {e}"))?;

    let current_version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| format!("读取归档数据库版本失败: {e}"))?;
    let latest_version = MIGRATIONS.last().map_or(0, |migration| migration.version);
    if current_version > latest_version {
        return Err(format!(
            "归档数据库版本 {current_version} 高于当前程序支持的 {latest_version}"
        ));
    }

    // 2.4.2 及更早版本没有 schema 版本，只在首次接管时兼容一次。
    if current_version == 0 {
        migrate_unversioned_gift_columns(conn)?;
    }

    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > current_version)
    {
        let tx = conn
            .transaction()
            .map_err(|e| format!("开启归档迁移事务失败: {e}"))?;
        tx.execute_batch(migration.sql)
            .map_err(|e| format!("执行归档迁移 {} 失败: {e}", migration.name))?;
        tx.pragma_update(None, "user_version", migration.version)
            .map_err(|e| format!("记录归档迁移版本失败: {e}"))?;
        tx.commit()
            .map_err(|e| format!("提交归档迁移 {} 失败: {e}", migration.name))?;
        log::info!(
            "Applied archive migration v{} ({})",
            migration.version,
            migration.name
        );
    }

    conn.execute_batch("PRAGMA optimize;")
        .map_err(|e| format!("优化归档数据库失败: {e}"))?;
    Ok(())
}

fn migrate_unversioned_gift_columns(conn: &Connection) -> Result<(), String> {
    let columns = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(gifts)")
            .map_err(|e| format!("读取旧归档礼物表结构失败: {e}"))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("读取旧归档礼物字段失败: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("读取旧归档礼物字段失败: {e}"))?;
        columns
    };

    for (column, sql) in [
        (
            "revenue_value",
            "ALTER TABLE gifts ADD COLUMN revenue_value INTEGER",
        ),
        (
            "blind_gift_id",
            "ALTER TABLE gifts ADD COLUMN blind_gift_id INTEGER",
        ),
        (
            "blind_gift_name",
            "ALTER TABLE gifts ADD COLUMN blind_gift_name TEXT",
        ),
        (
            "blind_gift_total_value",
            "ALTER TABLE gifts ADD COLUMN blind_gift_total_value INTEGER",
        ),
        (
            "batch_combo_id",
            "ALTER TABLE gifts ADD COLUMN batch_combo_id TEXT",
        ),
        (
            "combo_total_coin",
            "ALTER TABLE gifts ADD COLUMN combo_total_coin INTEGER",
        ),
        (
            "super_batch_gift_num",
            "ALTER TABLE gifts ADD COLUMN super_batch_gift_num INTEGER",
        ),
        (
            "combo_resources_id",
            "ALTER TABLE gifts ADD COLUMN combo_resources_id INTEGER",
        ),
        (
            "combo_stay_time",
            "ALTER TABLE gifts ADD COLUMN combo_stay_time INTEGER",
        ),
        (
            "show_batch_combo_send",
            "ALTER TABLE gifts ADD COLUMN show_batch_combo_send INTEGER",
        ),
    ] {
        if !columns.iter().any(|existing| existing == column) {
            conn.execute(sql, [])
                .map_err(|e| format!("迁移旧归档礼物字段 {column} 失败: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::initialize;
    use rusqlite::Connection;

    #[test]
    fn initializes_and_versions_archive_schema_idempotently() {
        let mut conn = Connection::open_in_memory().expect("in-memory database");
        initialize(&mut conn).expect("first migration");
        initialize(&mut conn).expect("idempotent migration");

        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, 3);
    }

    #[test]
    fn adopts_unversioned_archive_database() {
        let mut conn = Connection::open_in_memory().expect("in-memory database");
        conn.execute_batch(
            "CREATE TABLE gifts (
                id INTEGER PRIMARY KEY,
                session_id INTEGER NOT NULL,
                original_id TEXT NOT NULL,
                gift_id INTEGER NOT NULL,
                gift_name TEXT NOT NULL,
                gift_icon TEXT,
                num INTEGER NOT NULL,
                total_value INTEGER NOT NULL,
                is_paid INTEGER NOT NULL DEFAULT 0,
                user_uid INTEGER NOT NULL,
                user_name TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                guard_level INTEGER
            );",
        )
        .expect("legacy gifts table");

        initialize(&mut conn).expect("adopt legacy database");
        let has_revenue_value: bool = conn
            .prepare("PRAGMA table_info(gifts)")
            .expect("gift table info")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("gift columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("column names")
            .iter()
            .any(|column| column == "revenue_value");
        assert!(has_revenue_value);
    }
}
