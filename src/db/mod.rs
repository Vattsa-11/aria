use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::model::{Playlist, Track};
use crate::queue::Queue;
use crate::settings::{data_dir, RepeatMode};

const SCHEMA: &str = "
PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS tracks (
    id   INTEGER PRIMARY KEY,
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS queue_user (
    position INTEGER PRIMARY KEY,
    track_id INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS queue_context (
    position INTEGER PRIMARY KEY,
    track_id INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS queue_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS liked (
    position INTEGER PRIMARY KEY,
    track_id INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS playlists (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS playlist_tracks (
    playlist_id INTEGER NOT NULL,
    position    INTEGER NOT NULL,
    track_id    INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, position)
);
CREATE TABLE IF NOT EXISTS history (
    position INTEGER PRIMARY KEY,
    track_id INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS downloads (
    track_id INTEGER PRIMARY KEY,
    path     TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS lyrics (
    track_id INTEGER PRIMARY KEY,
    body     TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS features (
    track_id INTEGER PRIMARY KEY,
    bpm      REAL NOT NULL,
    energy   REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS plays (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    track_id   INTEGER NOT NULL,
    title      TEXT NOT NULL,
    artist     TEXT NOT NULL,
    duration   INTEGER NOT NULL,
    played_at  INTEGER NOT NULL
);
";

#[derive(Serialize, Deserialize, Default, Debug)]
struct QueueMeta {
    context_name: String,
    context_pos: Option<usize>,
    order: Vec<usize>,
    shuffle: bool,
    repeat: RepeatMode,
    rng: u64,
    current: Option<Track>,
    position_ms: u64,
}

#[derive(Debug, Default)]
pub struct SavedState {
    pub queue: Option<Queue>,
    pub current: Option<Track>,
    pub position_ms: u64,
}

static INITIALISED: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

pub fn integrity() -> Result<String, String> {
    let conn = open()?;
    conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0)).map_err(|err| err.to_string())
}

fn open() -> Result<Connection, String> {
    #[cfg(target_arch = "wasm32")]
    let path = std::path::PathBuf::from("library.db");
    #[cfg(not(target_arch = "wasm32"))]
    let path = {
        let dir = data_dir();
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        dir.join("library.db")
    };
    let conn = Connection::open(&path).map_err(|err| err.to_string())?;
    #[cfg(target_arch = "wasm32")]
    conn.execute_batch("PRAGMA synchronous=off;").map_err(|err| err.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|err| err.to_string())?;
    let mut ready = INITIALISED.lock().unwrap_or_else(|err| err.into_inner());
    if !ready.contains(&path) {
        conn.execute_batch(SCHEMA).map_err(|err| err.to_string())?;
        ready.push(path);
    }
    Ok(conn)
}

#[derive(Debug, Default, Clone)]
pub struct ListeningStats {
    pub top_tracks: Vec<(String, String, u32, u64)>,
    pub top_artists: Vec<(String, u32, u64)>,
    pub months: Vec<(String, u64)>,
    pub total_plays: u32,
    pub total_seconds: u64,
}

pub fn log_play(track: &Track) {
    let Ok(conn) = open() else {
        return;
    };
    let played_at = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|span| span.as_secs() as i64)
        .unwrap_or(0);
    let _ = conn.execute(
        "INSERT INTO plays (track_id, title, artist, duration, played_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            track.id as i64,
            track.title,
            track.artist,
            track.duration.map(|d| d.as_secs() as i64).unwrap_or(0),
            played_at
        ],
    );
    let _ = conn.execute(
        "DELETE FROM plays WHERE id NOT IN (SELECT id FROM plays ORDER BY id DESC LIMIT 20000)",
        [],
    );
}

pub fn listening_stats() -> Result<ListeningStats, String> {
    let conn = open()?;
    let mut stats = ListeningStats::default();

    let mut statement = conn
        .prepare(
            "SELECT title, artist, COUNT(*) AS plays, SUM(duration) AS seconds
             FROM plays
             GROUP BY title, artist
             ORDER BY plays DESC, seconds DESC
             LIMIT 10",
        )
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u32,
                row.get::<_, i64>(3).unwrap_or(0) as u64,
            ))
        })
        .map_err(|err| err.to_string())?;
    for row in rows.flatten() {
        stats.top_tracks.push(row);
    }

    let mut statement = conn
        .prepare(
            "SELECT artist, COUNT(*) AS plays, SUM(duration) AS seconds
             FROM plays
             GROUP BY artist
             ORDER BY plays DESC, seconds DESC
             LIMIT 10",
        )
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u32,
                row.get::<_, i64>(2).unwrap_or(0) as u64,
            ))
        })
        .map_err(|err| err.to_string())?;
    for row in rows.flatten() {
        stats.top_artists.push(row);
    }

    let mut statement = conn
        .prepare("SELECT played_at, duration FROM plays ORDER BY played_at DESC LIMIT 20000")
        .map_err(|err| err.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1).unwrap_or(0) as u64,
            ))
        })
        .map_err(|err| err.to_string())?;
    let mut totals: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for (played_at, seconds) in rows.flatten() {
        stats.total_plays = stats.total_plays.saturating_add(1);
        stats.total_seconds = stats.total_seconds.saturating_add(seconds);
        *totals.entry(month_key(played_at)).or_default() += seconds;
    }
    stats.months = totals.into_iter().rev().take(12).collect();
    Ok(stats)
}

fn month_key(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let (year, month, _) = civil_from_days(days);
    format!("{year:04}-{month:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

pub fn load_features(track_id: u64) -> Option<crate::pace::Features> {
    let conn = open().ok()?;
    conn.query_row(
        "SELECT bpm, energy FROM features WHERE track_id = ?1",
        params![track_id as i64],
        |row| {
            Ok(crate::pace::Features {
                bpm: row.get::<_, f64>(0)? as f32,
                energy: row.get::<_, f64>(1)? as f32,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

pub fn save_features(track_id: u64, features: crate::pace::Features) -> Result<(), String> {
    let conn = open()?;
    conn.execute(
        "INSERT OR REPLACE INTO features (track_id, bpm, energy) VALUES (?1, ?2, ?3)",
        params![track_id as i64, features.bpm as f64, features.energy as f64],
    )
    .map(|_| ())
    .map_err(|err| err.to_string())
}

pub fn load_lyrics(track_id: u64) -> Option<String> {
    let conn = open().ok()?;
    conn.query_row(
        "SELECT body FROM lyrics WHERE track_id = ?1",
        params![track_id as i64],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn save_lyrics(track_id: u64, body: &str) -> Result<(), String> {
    let conn = open()?;
    conn.execute(
        "INSERT INTO lyrics (track_id, body) VALUES (?1, ?2)
         ON CONFLICT(track_id) DO UPDATE SET body = excluded.body",
        params![track_id as i64, body],
    )
    .map(|_| ())
    .map_err(|err| err.to_string())
}

fn track_to_json(track: &Track) -> String {
    serde_json::to_string(track).unwrap_or_default()
}

fn track_from_json(raw: &str) -> Option<Track> {
    serde_json::from_str(raw).ok()
}

pub fn save_state(queue: &Queue, current: Option<&Track>, position_ms: u64) -> Result<(), String> {
    let mut conn = open()?;
    save_state_on(&mut conn, queue, current, position_ms)
}

pub fn save_state_on(
    conn: &mut Connection,
    queue: &Queue,
    current: Option<&Track>,
    position_ms: u64,
) -> Result<(), String> {
    let tx = conn
        .transaction()
        .map_err(|err| err.to_string())
        .map_err(|err| err.to_string())?;

    let upsert = |track: &Track| -> Result<(), String> {
        tx.execute(
            "INSERT INTO tracks (id, json) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![track.id as i64, track_to_json(track)],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    };

    let user_ids: Vec<i64> = queue.user.iter().map(|track| track.id as i64).collect();
    if !ordered_rows_match(&tx, "queue_user", &user_ids)? {
        tx.execute("DELETE FROM queue_user", [])
            .map_err(|err| err.to_string())?;
        for (position, track) in queue.user.iter().enumerate() {
            upsert(track)?;
            tx.execute(
                "INSERT INTO queue_user (position, track_id) VALUES (?1, ?2)",
                params![position as i64, track.id as i64],
            )
            .map_err(|err| err.to_string())?;
        }
    }
    let context_ids: Vec<i64> = queue.context.iter().map(|track| track.id as i64).collect();
    if !ordered_rows_match(&tx, "queue_context", &context_ids)? {
        tx.execute("DELETE FROM queue_context", [])
            .map_err(|err| err.to_string())?;
        for (position, track) in queue.context.iter().enumerate() {
            upsert(track)?;
            tx.execute(
                "INSERT INTO queue_context (position, track_id) VALUES (?1, ?2)",
                params![position as i64, track.id as i64],
            )
            .map_err(|err| err.to_string())?;
        }
    }

    if let Some(track) = current {
        upsert(track)?;
    }

    let meta = QueueMeta {
        context_name: queue.context_name.clone(),
        context_pos: queue.context_pos,
        order: queue.order.clone(),
        shuffle: queue.shuffle,
        repeat: queue.repeat,
        rng: queue.rng,
        current: current.cloned(),
        position_ms,
    };
    let meta_json = serde_json::to_string(&meta).map_err(|err| err.to_string())?;
    tx.execute(
        "INSERT INTO queue_meta (key, value) VALUES ('queue', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![meta_json],
    )
    .map_err(|err| err.to_string())?;

    tx.commit().map_err(|err| err.to_string())
}

pub fn load_state() -> Result<SavedState, String> {
    let conn = open()?;
    load_state_on(&conn)
}

pub fn load_state_on(conn: &Connection) -> Result<SavedState, String> {
    let meta_json: Option<String> = conn
        .query_row(
            "SELECT value FROM queue_meta WHERE key = 'queue'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;

    let Some(meta_json) = meta_json else {
        return Ok(SavedState::default());
    };
    let meta: QueueMeta =
        serde_json::from_str(&meta_json).map_err(|err| err.to_string())?;

    let load_tracks = |table: &str| -> Result<Vec<Track>, String> {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT t.json FROM {table} q
                 JOIN tracks t ON t.id = q.track_id
                 ORDER BY q.position"
            ))
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|err| err.to_string())?;
        let mut tracks = Vec::new();
        for row in rows {
            let raw = row.map_err(|err| err.to_string())?;
            if let Some(track) = track_from_json(&raw) {
                tracks.push(track);
            }
        }
        Ok(tracks)
    };

    let user: std::collections::VecDeque<Track> = load_tracks("queue_user")?.into();
    let context = load_tracks("queue_context")?;

    let mut queue = Queue::new(meta.shuffle, meta.repeat);
    queue.user = user;
    queue.context = context;
    queue.context_name = meta.context_name;
    queue.context_pos = meta.context_pos;
    queue.order = meta.order;
    queue.rng = meta.rng;
    if queue.order.len() != queue.context.len() {
        queue.rebuild_order();
    }

    Ok(SavedState {
        queue: Some(queue),
        current: meta.current,
        position_ms: meta.position_ms,
    })
}

fn ordered_rows_match(
    tx: &rusqlite::Transaction,
    table: &str,
    ids: &[i64],
) -> Result<bool, String> {
    let mut stmt = tx
        .prepare(&format!("SELECT track_id FROM {table} ORDER BY position"))
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, i64>(0))
        .map_err(|err| err.to_string())?;
    let mut saved = Vec::new();
    for row in rows {
        saved.push(row.map_err(|err| err.to_string())?);
    }
    Ok(saved == ids)
}

fn upsert_track(tx: &rusqlite::Transaction, track: &Track) -> Result<(), String> {
    tx.execute(
        "INSERT INTO tracks (id, json) VALUES (?1, ?2)
         ON CONFLICT(id) DO UPDATE SET json = excluded.json",
        params![track.id as i64, track_to_json(track)],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn load_track_list(
    conn: &Connection,
    table: &str,
    key_col: &str,
    key_val: Option<i64>,
) -> Result<Vec<Track>, String> {
    let sql = match key_val {
        Some(_) => format!(
            "SELECT t.json FROM {table} q JOIN tracks t ON t.id = q.track_id
             WHERE q.{key_col} = ?1 ORDER BY q.position"
        ),
        None => format!(
            "SELECT t.json FROM {table} q JOIN tracks t ON t.id = q.track_id
             ORDER BY q.position"
        ),
    };
    let mut stmt = conn.prepare(&sql).map_err(|err| err.to_string())?;
    let mut tracks = Vec::new();
    match key_val {
        Some(val) => {
            let rows = stmt
                .query_map(params![val], |row| row.get::<_, String>(0))
                .map_err(|err| err.to_string())?;
            for row in rows {
                let raw = row.map_err(|err| err.to_string())?;
                if let Some(track) = track_from_json(&raw) {
                    tracks.push(track);
                }
            }
        }
        None => {
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|err| err.to_string())?;
            for row in rows {
                let raw = row.map_err(|err| err.to_string())?;
                if let Some(track) = track_from_json(&raw) {
                    tracks.push(track);
                }
            }
        }
    }
    Ok(tracks)
}

pub fn load_liked() -> Result<Vec<Track>, String> {
    let conn = open()?;
    load_liked_on(&conn)
}

pub fn load_liked_on(conn: &Connection) -> Result<Vec<Track>, String> {
    load_track_list(conn, "liked", "unused", None)
}

pub fn save_liked(tracks: &[Track]) -> Result<(), String> {
    let mut conn = open()?;
    save_liked_on(&mut conn, tracks)
}

pub fn save_liked_on(conn: &mut Connection, tracks: &[Track]) -> Result<(), String> {
    let tx = conn.transaction().map_err(|err| err.to_string())?;
    sync_ordered_list(&tx, "liked", tracks)?;
    tx.commit().map_err(|err| err.to_string())
}

fn sync_ordered_list(
    tx: &rusqlite::Transaction,
    table: &str,
    tracks: &[Track],
) -> Result<(), String> {
    let mut existing: Vec<(i64, i64)> = Vec::new();
    {
        let mut stmt = tx
            .prepare(&format!(
                "SELECT position, track_id FROM {table} ORDER BY position"
            ))
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .map_err(|err| err.to_string())?;
        for row in rows {
            existing.push(row.map_err(|err| err.to_string())?);
        }
    }
    for (position, track_id) in existing {
        let wanted = tracks.get(position as usize).map(|track| track.id as i64);
        if wanted == Some(track_id) {
            continue;
        }
        tx.execute(
            &format!("DELETE FROM {table} WHERE position = ?1"),
            params![position],
        )
        .map_err(|err| err.to_string())?;
    }
    for (position, track) in tracks.iter().enumerate() {
        upsert_track(tx, track)?;
        tx.execute(
            &format!(
                "INSERT INTO {table} (position, track_id) VALUES (?1, ?2)
                 ON CONFLICT(position) DO UPDATE SET track_id = excluded.track_id"
            ),
            params![position as i64, track.id as i64],
        )
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn load_playlists() -> Result<Vec<Playlist>, String> {
    let conn = open()?;
    load_playlists_on(&conn)
}

pub fn load_playlists_on(conn: &Connection) -> Result<Vec<Playlist>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name FROM playlists ORDER BY rowid")
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|err| err.to_string())?;
    let mut playlists = Vec::new();
    for row in rows {
        let (id, name) = row.map_err(|err| err.to_string())?;
        let tracks = load_track_list(conn, "playlist_tracks", "playlist_id", Some(id))?;
        playlists.push(Playlist {
            id: id as u64,
            name,
            tracks,
        });
    }
    Ok(playlists)
}

pub fn save_playlists(playlists: &[Playlist]) -> Result<(), String> {
    let mut conn = open()?;
    save_playlists_on(&mut conn, playlists)
}

pub fn save_playlists_on(conn: &mut Connection, playlists: &[Playlist]) -> Result<(), String> {
    let tx = conn.transaction().map_err(|err| err.to_string())?;
    let mut existing: Vec<i64> = Vec::new();
    {
        let mut stmt = tx
            .prepare("SELECT id FROM playlists")
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|err| err.to_string())?;
        for row in rows {
            existing.push(row.map_err(|err| err.to_string())?);
        }
    }
    for id in existing {
        if playlists.iter().any(|playlist| playlist.id as i64 == id) {
            continue;
        }
        tx.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
            params![id],
        )
        .map_err(|err| err.to_string())?;
        tx.execute("DELETE FROM playlists WHERE id = ?1", params![id])
            .map_err(|err| err.to_string())?;
    }
    for playlist in playlists {
        tx.execute(
            "INSERT INTO playlists (id, name) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name",
            params![playlist.id as i64, playlist.name],
        )
        .map_err(|err| err.to_string())?;
        if playlist_tracks_match(&tx, playlist)? {
            continue;
        }
        tx.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist.id as i64],
        )
        .map_err(|err| err.to_string())?;
        for (position, track) in playlist.tracks.iter().enumerate() {
            upsert_track(&tx, track)?;
            tx.execute(
                "INSERT INTO playlist_tracks (playlist_id, position, track_id) VALUES (?1, ?2, ?3)",
                params![playlist.id as i64, position as i64, track.id as i64],
            )
            .map_err(|err| err.to_string())?;
        }
    }
    tx.commit().map_err(|err| err.to_string())
}

fn playlist_tracks_match(tx: &rusqlite::Transaction, playlist: &Playlist) -> Result<bool, String> {
    let mut stmt = tx
        .prepare(
            "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map(params![playlist.id as i64], |row| row.get::<_, i64>(0))
        .map_err(|err| err.to_string())?;
    let mut saved = Vec::new();
    for row in rows {
        saved.push(row.map_err(|err| err.to_string())?);
    }
    if saved.len() != playlist.tracks.len() {
        return Ok(false);
    }
    Ok(saved
        .iter()
        .zip(playlist.tracks.iter())
        .all(|(saved_id, track)| *saved_id == track.id as i64))
}

pub fn load_downloads() -> Result<Vec<Track>, String> {
    let conn = open()?;
    let mut stmt = conn
        .prepare(
            "SELECT t.json FROM downloads d JOIN tracks t ON t.id = d.track_id
             ORDER BY d.rowid",
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|err| err.to_string())?;
    let mut tracks = Vec::new();
    for row in rows {
        let raw = row.map_err(|err| err.to_string())?;
        if let Some(track) = track_from_json(&raw) {
            tracks.push(track);
        }
    }
    Ok(tracks)
}

pub fn save_download(track: &Track, path: &std::path::Path) -> Result<(), String> {
    let conn = open()?;
    let mut conn = conn;
    let tx = conn.transaction().map_err(|err| err.to_string())?;
    upsert_track(&tx, track)?;
    tx.execute(
        "INSERT INTO downloads (track_id, path) VALUES (?1, ?2)
         ON CONFLICT(track_id) DO UPDATE SET path = excluded.path",
        params![track.id as i64, path.to_string_lossy().to_string()],
    )
    .map_err(|err| err.to_string())?;
    tx.commit().map_err(|err| err.to_string())
}

pub fn delete_download(track_id: u64) -> Result<(), String> {
    let conn = open()?;
    conn.execute(
        "DELETE FROM downloads WHERE track_id = ?1",
        params![track_id as i64],
    )
    .map(|_| ())
    .map_err(|err| err.to_string())
}

pub fn clear_downloads() -> Result<(), String> {
    let conn = open()?;
    conn.execute("DELETE FROM downloads", [])
        .map(|_| ())
        .map_err(|err| err.to_string())
}

pub fn load_history() -> Result<Vec<Track>, String> {
    let conn = open()?;
    let mut stmt = conn
        .prepare(
            "SELECT t.json FROM history h JOIN tracks t ON t.id = h.track_id
             ORDER BY h.rowid",
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|err| err.to_string())?;
    let mut tracks = Vec::new();
    for row in rows {
        let raw = row.map_err(|err| err.to_string())?;
        if let Some(track) = track_from_json(&raw) {
            tracks.push(track);
        }
    }
    Ok(tracks)
}

pub fn save_history(tracks: &[Track]) -> Result<(), String> {
    let mut conn = open()?;
    save_history_on(&mut conn, tracks)
}

pub fn save_history_on(conn: &mut Connection, tracks: &[Track]) -> Result<(), String> {
    let tx = conn.transaction().map_err(|err| err.to_string())?;
    sync_ordered_list(&tx, "history", tracks)?;
    tx.commit().map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(SCHEMA).expect("schema");
        conn
    }

    fn track(title: &str) -> Track {
        Track::yt(
            &format!("aria-test-{title}"),
            title,
            "Artist",
            Some(std::time::Duration::from_secs(90)),
        )
    }

    fn titles(tracks: &[Track]) -> Vec<String> {
        tracks.iter().map(|t| t.title.clone()).collect()
    }

    #[test]
    fn playlist_create_reorder_and_delete_persist() {
        let mut conn = mem();
        let mut playlist = Playlist::new("Road trip");
        playlist.tracks = vec![track("a"), track("b"), track("c")];
        save_playlists_on(&mut conn, std::slice::from_ref(&playlist)).expect("save");

        let loaded = load_playlists_on(&conn).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Road trip");
        assert_eq!(titles(&loaded[0].tracks), ["a", "b", "c"]);

        let mut reordered = playlist.clone();
        let last = reordered.tracks.remove(2);
        reordered.tracks.insert(0, last);
        save_playlists_on(&mut conn, &[reordered]).expect("resave");
        let loaded = load_playlists_on(&conn).expect("reload");
        assert_eq!(titles(&loaded[0].tracks), ["c", "a", "b"]);

        save_playlists_on(&mut conn, &[]).expect("delete");
        assert!(load_playlists_on(&conn).expect("final load").is_empty());
    }

    #[test]
    fn liked_toggle_roundtrip() {
        let mut conn = mem();
        save_liked_on(&mut conn, &[track("x"), track("y")]).expect("save liked");
        assert_eq!(titles(&load_liked_on(&conn).expect("load liked")), ["x", "y"]);

        save_liked_on(&mut conn, &[track("y")]).expect("unlike x");
        assert_eq!(titles(&load_liked_on(&conn).expect("reload liked")), ["y"]);

        save_liked_on(&mut conn, &[]).expect("unlike all");
        assert!(load_liked_on(&conn).expect("empty liked").is_empty());
    }

    #[test]
    fn queue_persistence_roundtrip() {
        let mut conn = mem();
        let mut queue = Queue::new(true, RepeatMode::All);
        queue.set_context(
            vec![track("one"), track("two"), track("three")],
            "Album",
        );
        queue.push_user(track("queued"));
        queue.mark_context_playing(1);
        let current = queue.context[1].clone();

        save_state_on(&mut conn, &queue, Some(&current), 45_000).expect("save state");
        let saved = load_state_on(&conn).expect("load state");
        let restored = saved.queue.expect("queue restored");

        assert_eq!(
            restored.user.iter().map(|t| t.title.clone()).collect::<Vec<_>>(),
            ["queued"]
        );
        assert_eq!(titles(&restored.context), ["one", "two", "three"]);
        assert_eq!(restored.context_name, "Album");
        assert_eq!(restored.context_pos, Some(1));
        assert_eq!(restored.order, queue.order);
        assert!(restored.shuffle);
        assert_eq!(restored.repeat, RepeatMode::All);
        assert_eq!(saved.current.map(|t| t.title), Some("two".to_string()));
        assert_eq!(saved.position_ms, 45_000);
    }

    #[test]
    fn month_keys_are_calendar_months() {
        assert_eq!(month_key(0), "1970-01");
        assert_eq!(month_key(1_704_067_200), "2024-01");
        assert_eq!(month_key(1_706_745_600), "2024-02");
    }

    #[test]
    fn lyrics_are_cached_per_track() {
        crate::settings::test_data_dir();
        let unique = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|span| span.subsec_nanos() as u64)
            .unwrap_or(7);
        let id = 900_000_000 + unique;
        assert!(load_lyrics(id).is_none());
        save_lyrics(id, "[00:01.00]hello").expect("save");
        assert_eq!(load_lyrics(id).as_deref(), Some("[00:01.00]hello"));
        save_lyrics(id, "[00:02.00]bye").expect("save");
        assert_eq!(load_lyrics(id).as_deref(), Some("[00:02.00]bye"));
    }

    #[test]
    fn download_rows_roundtrip() {
        crate::settings::test_data_dir();
        let track = Track::yt("aria-test-db-download", "Roundtrip", "Artist", None);
        let path = crate::downloads::dir().join("roundtrip.m4a");

        save_download(&track, &path).expect("save download");
        let loaded = load_downloads().expect("load downloads");
        assert!(
            loaded.iter().any(|saved| saved.id == track.id),
            "downloaded track should be listed"
        );

        delete_download(track.id).expect("delete download");
        let loaded = load_downloads().expect("load downloads");
        assert!(
            !loaded.iter().any(|saved| saved.id == track.id),
            "deleted track should be gone"
        );
    }

    #[test]
    fn features_round_trip_and_replace() {
        crate::settings::test_data_dir();
        let id = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        assert!(load_features(id).is_none());
        save_features(id, crate::pace::Features { bpm: 101.5, energy: 0.42 }).expect("save");
        let loaded = load_features(id).expect("load");
        assert!((loaded.bpm - 101.5).abs() < 0.01 && (loaded.energy - 0.42).abs() < 0.01);
        save_features(id, crate::pace::Features { bpm: 70.0, energy: 0.1 }).expect("replace");
        assert!((load_features(id).unwrap().bpm - 70.0).abs() < 0.01);
    }
}
