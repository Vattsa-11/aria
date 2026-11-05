use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::app::Page;
use crate::settings::data_dir;

pub const DEFAULT_PORT: u16 = 47474;
const REPLY_TIMEOUT: Duration = Duration::from_secs(25);
const MAX_LINE: usize = 64 * 1024;

pub struct Call {
    pub cmd: String,
    pub args: Value,
    pub reply: Sender<Value>,
}

pub struct AgentHandle {
    pub calls: Receiver<Call>,
    pub port: u16,
    pub token_path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Drop for AgentHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

pub fn agent_dir() -> PathBuf {
    data_dir().join("agent")
}

pub fn token_path() -> PathBuf {
    agent_dir().join("token")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Read,
    Control,
    Destructive,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Read => "read",
            Scope::Control => "control",
            Scope::Destructive => "destructive",
        }
    }
}

pub struct CommandInfo {
    pub name: &'static str,
    pub scope: Scope,
    pub about: &'static str,
    pub args: &'static [&'static str],
}

const fn cmd(name: &'static str, scope: Scope, about: &'static str, args: &'static [&'static str]) -> CommandInfo {
    CommandInfo { name, scope, about, args }
}

use Scope::{Control, Destructive, Read as Reads};

pub const COMMANDS: &[CommandInfo] = &[
    cmd("ping", Reads, "Check the connection and get the app version", &[]),
    cmd("state", Reads, "Full player, page and window state", &[]),
    cmd("commands", Reads, "List every command with its scope and argument schema", &[]),
    cmd("health", Reads, "Run live health checks (database, audio device, providers, latency, folders, updates, playback success); pass run:true to start a fresh run", &["run?:boolean"]),
    cmd("history", Reads, "Recently played songs, newest first", &["limit?:number"]),
    cmd("events", Reads, "Changes since a sequence number (track starts, errors); poll with the last seq you saw", &["since?:number"]),
    cmd("audit", Reads, "Which agent commands ran recently and whether they worked", &["limit?:number"]),
    cmd("play", Control, "Start or resume playback", &[]),
    cmd("pause", Control, "Pause playback", &[]),
    cmd("toggle", Control, "Toggle play and pause", &[]),
    cmd("next", Control, "Skip to the next song", &[]),
    cmd("prev", Control, "Go to the previous song or restart this one", &[]),
    cmd("seek", Control, "Seek within the current song", &["secs:number"]),
    cmd("volume", Control, "Set volume between 0 and 1", &["value:number"]),
    cmd("like", Control, "Like or unlike the current song", &[]),
    cmd("shuffle", Control, "Toggle shuffle", &[]),
    cmd("repeat", Control, "Cycle repeat off, all, one", &[]),
    cmd("navigate", Control, "Open a page: home, library, search, playlists, liked, history, stats, downloads, import, settings, more", &["page:string"]),
    cmd("panel", Control, "Open or close queue, lyrics, now_playing or miniplayer", &["name:string", "open?:boolean"]),
    cmd("search", Control, "Search for songs", &["query:string"]),
    cmd("results", Reads, "Current search results", &[]),
    cmd("play_result", Control, "Play a search result by index", &["index:number"]),
    cmd("mood", Control, "Start a mood mix: chill, focus, workout, party, sad", &["name:string"]),
    cmd("queue", Reads, "The current queue", &[]),
    cmd("queue_add", Control, "Add a search result to the queue", &["index:number"]),
    cmd("queue_remove", Destructive, "Remove a song from the queue", &["index:number"]),
    cmd("queue_clear", Destructive, "Clear the queue", &[]),
    cmd("playlists", Reads, "List playlists", &[]),
    cmd("playlist", Reads, "Songs in one playlist", &["index:number"]),
    cmd("playlist_create", Control, "Create a playlist", &["name:string"]),
    cmd("playlist_add", Control, "Add a search result to a playlist", &["playlist:number", "result_index:number"]),
    cmd("playlist_play", Control, "Play a playlist", &["index:number"]),
    cmd("playlist_delete", Destructive, "Delete a playlist", &["index:number"]),
    cmd("list", Reads, "List songs in a named collection such as liked, history or downloads", &["name?:string"]),
    cmd("play_list", Control, "Play a named collection", &["name?:string", "index?:number"]),
    cmd("download", Control, "Download a search result for offline use", &["index:number"]),
    cmd("download_cancel", Control, "Cancel a download", &["id:number"]),
    cmd("download_delete", Destructive, "Delete a downloaded file", &["id:number"]),
    cmd("lyrics", Reads, "Lyrics for the current song", &[]),
    cmd("lyrics_refresh", Control, "Fetch the lyrics again", &[]),
    cmd("import_url", Control, "Import a playlist link", &["url:string"]),
    cmd("region", Control, "Set the trending region", &["mode:string", "language?:string"]),
    cmd("sleep", Control, "Set the sleep timer", &["minutes?:number"]),
    cmd("settings", Reads, "All settings", &[]),
    cmd("set_setting", Destructive, "Change a setting", &["key:string", "value:any"]),
    cmd("window", Control, "Resize the main window", &["width:number", "height:number"]),
    cmd("preview", Control, "Preview a width and height without resizing the window", &["width?:number", "height?:number"]),
    cmd("layer_at", Reads, "Which layer is at a screen point", &["x:number", "y:number"]),
    cmd("mini_size", Control, "Resize the miniplayer", &["width:number", "height:number"]),
    cmd("live", Destructive, "Write live style overrides to disk", &["overrides?:object"]),
    cmd("logs", Reads, "Recent log lines", &["count?:number"]),
    cmd("diagnose", Reads, "Analyse the log for known problems", &[]),
    cmd("screenshot", Reads, "Save a screenshot; viewport:\"mini\" for the miniplayer", &["viewport?:string"]),
];

pub fn scope_of(name: &str) -> Option<Scope> {
    COMMANDS.iter().find(|info| info.name == name).map(|info| info.scope)
}

fn schema(args: &[&str]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for arg in args {
        let (name, kind) = arg.split_once(':').unwrap_or((arg, "string"));
        let optional = name.ends_with('?');
        let name = name.trim_end_matches('?');
        properties.insert(name.to_string(), if kind == "any" { json!({}) } else { json!({ "type": kind }) });
        if !optional {
            required.push(name.to_string());
        }
    }
    json!({ "type": "object", "properties": properties, "required": required })
}

pub fn describe() -> Value {
    Value::Array(
        COMMANDS
            .iter()
            .map(|info| json!({ "name": info.name, "scope": info.scope.label(), "description": info.about, "args": schema(info.args) }))
            .collect(),
    )
}

fn random_hex() -> String {
    let state = std::collections::hash_map::RandomState::new();
    let mut out = String::new();
    for round in 0..2u64 {
        let mut hasher = state.build_hasher();
        hasher.write_u64(round);
        hasher.write_u128(
            web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out
}

pub fn load_or_create_token() -> Result<String, String> {
    let path = token_path();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim().to_string();
        if existing.len() >= 32 {
            return Ok(existing);
        }
    }
    std::fs::create_dir_all(agent_dir()).map_err(|err| err.to_string())?;
    let token = random_hex();
    std::fs::write(&path, &token).map_err(|err| err.to_string())?;
    Ok(token)
}

pub fn tokens_match(expected: &str, given: &str) -> bool {
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn parse_line(line: &str) -> Result<(Value, String, Value, String), String> {
    let value: Value = serde_json::from_str(line).map_err(|err| format!("not valid JSON: {err}"))?;
    let object = value.as_object().ok_or("a request must be a JSON object")?;
    let cmd = object
        .get("cmd")
        .and_then(Value::as_str)
        .filter(|cmd| !cmd.is_empty())
        .ok_or("missing \"cmd\"")?
        .to_string();
    let token = object.get("token").and_then(Value::as_str).unwrap_or("").to_string();
    let id = object.get("id").cloned().unwrap_or(Value::Null);
    let args = object.get("args").cloned().unwrap_or(Value::Null);
    Ok((id, cmd, args, token))
}

pub fn page_name(page: Page) -> &'static str {
    match page {
        Page::Home => "home",
        Page::Library => "library",
        Page::Search => "search",
        Page::Playlists => "playlists",
        Page::Liked => "liked",
        Page::History => "history",
        Page::Stats => "stats",
        Page::Downloads => "downloads",
        Page::Import => "import",
        Page::PlaylistDetail(_) => "playlist",
        Page::Settings => "settings",
        Page::More => "more",
    }
}

pub fn parse_page(name: &str) -> Option<Page> {
    Some(match name.trim().to_lowercase().as_str() {
        "home" => Page::Home,
        "library" => Page::Library,
        "search" => Page::Search,
        "playlists" => Page::Playlists,
        "liked" => Page::Liked,
        "history" => Page::History,
        "stats" => Page::Stats,
        "downloads" => Page::Downloads,
        "import" => Page::Import,
        "settings" => Page::Settings,
        "more" => Page::More,
        _ => return None,
    })
}

pub fn tail_lines(text: &str, count: usize) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(count)..].iter().map(|line| line.to_string()).collect()
}

fn respond(stream: &mut TcpStream, id: &Value, outcome: Result<Value, String>) -> bool {
    let body = match outcome {
        Ok(data) => json!({ "id": id, "ok": true, "data": data }),
        Err(error) => json!({ "id": id, "ok": false, "error": error }),
    };
    writeln!(stream, "{body}").is_ok()
}

fn serve(stream: TcpStream, token: String, calls: Sender<Call>, ctx: egui::Context) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match reader.by_ref().take(MAX_LINE as u64).read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (id, cmd, args, given) = match parse_line(line) {
            Ok(parsed) => parsed,
            Err(error) => {
                if !respond(&mut writer, &Value::Null, Err(error)) {
                    break;
                }
                continue;
            }
        };
        if !tokens_match(&token, &given) {
            let _ = respond(&mut writer, &id, Err("bad or missing token".to_string()));
            break;
        }
        let (reply_tx, reply_rx) = channel();
        if calls.send(Call { cmd, args, reply: reply_tx }).is_err() {
            break;
        }
        ctx.request_repaint();
        let outcome = match reply_rx.recv_timeout(REPLY_TIMEOUT) {
            Ok(value) => match value.get("error").and_then(Value::as_str) {
                Some(error) => Err(error.to_string()),
                None => Ok(value),
            },
            Err(_) => Err("the app did not answer in time".to_string()),
        };
        if !respond(&mut writer, &id, outcome) {
            break;
        }
    }
    let _ = writer.shutdown(Shutdown::Both);
}

use std::io::Read;

pub fn start(ctx: egui::Context, port: u16) -> Result<AgentHandle, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|err| format!("cannot listen on 127.0.0.1:{port}: {err}"))?;
    let token = load_or_create_token()?;
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stopper = stop.clone();
    let _ = std::thread::Builder::new().name("aria-agent".into()).spawn(move || {
        for stream in listener.incoming() {
            if stopper.load(Ordering::Relaxed) {
                break;
            }
            let Ok(stream) = stream else {
                continue;
            };
            let (token, calls, ctx) = (token.clone(), tx.clone(), ctx.clone());
            let _ = std::thread::Builder::new()
                .name("aria-agent-conn".into())
                .spawn(move || serve(stream, token, calls, ctx));
        }
    });
    Ok(AgentHandle { calls: rx, port, token_path: token_path(), stop })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_need_a_command_and_carry_optional_id_args_and_token() {
        let (id, cmd, args, token) = parse_line(r#"{"id":7,"cmd":"seek","args":{"secs":30},"token":"abc"}"#).unwrap();
        assert_eq!((id, cmd.as_str(), token.as_str()), (json!(7), "seek", "abc"));
        assert_eq!(args["secs"], 30);
        let (id, cmd, args, token) = parse_line(r#"{"cmd":"state"}"#).unwrap();
        assert_eq!((id, cmd.as_str(), args, token.as_str()), (Value::Null, "state", Value::Null, ""));
        assert!(parse_line("not json").is_err());
        assert!(parse_line("[1,2]").is_err());
        assert!(parse_line(r#"{"cmd":""}"#).is_err());
        assert!(parse_line(r#"{"id":1}"#).is_err());
    }

    #[test]
    fn tokens_are_compared_exactly() {
        assert!(tokens_match("abcdef", "abcdef"));
        assert!(!tokens_match("abcdef", "abcdeg"));
        assert!(!tokens_match("abcdef", "abcde"));
        assert!(!tokens_match("abcdef", ""));
        assert_eq!(random_hex().len(), 32);
        assert_ne!(random_hex(), random_hex());
    }

    #[test]
    fn page_names_round_trip() {
        for name in ["home", "library", "search", "playlists", "liked", "history", "stats", "downloads", "import", "settings", "more"] {
            assert_eq!(page_name(parse_page(name).unwrap()), name);
        }
        assert_eq!(parse_page(" HOME "), Some(Page::Home));
        assert_eq!(parse_page("nowhere"), None);
        assert_eq!(page_name(Page::PlaylistDetail(2)), "playlist");
    }

    #[test]
    fn log_tails_keep_the_newest_lines() {
        assert_eq!(tail_lines("a\nb\nc\nd", 2), vec!["c", "d"]);
        assert_eq!(tail_lines("a", 5), vec!["a"]);
        assert!(tail_lines("", 3).is_empty());
    }

    #[test]
    fn the_server_answers_over_tcp_and_rejects_a_wrong_token() {
        crate::settings::test_data_dir();
        let ctx = egui::Context::default();
        let port = 47900 + (std::process::id() % 90) as u16;
        let handle = start(ctx, port).expect("start");
        let token = load_or_create_token().expect("token");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        writeln!(stream, "{}", json!({"id": 1, "cmd": "ping", "token": token})).unwrap();
        let call = handle.calls.recv_timeout(Duration::from_secs(5)).expect("call arrives");
        let _ = call.reply.send(json!({ "echo": call.cmd }));
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["data"]["echo"], "ping");
        let mut bad = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        bad.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        writeln!(bad, "{}", json!({"id": 2, "cmd": "state", "token": "wrong"})).unwrap();
        let mut line = String::new();
        BufReader::new(bad).read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["ok"], false);
        assert!(reply["error"].as_str().unwrap().contains("token"));
    }

    #[test]
    fn every_command_has_a_scope_and_destructive_ones_are_marked() {
        assert_eq!(scope_of("state"), Some(Scope::Read));
        assert_eq!(scope_of("next"), Some(Scope::Control));
        assert_eq!(scope_of("queue_clear"), Some(Scope::Destructive));
        assert_eq!(scope_of("nope"), None);
        let described = describe();
        let seek = described.as_array().unwrap().iter().find(|c| c["name"] == "seek").unwrap();
        assert_eq!(seek["args"]["required"][0], "secs");
        assert_eq!(seek["args"]["properties"]["secs"]["type"], "number");
    }
}
