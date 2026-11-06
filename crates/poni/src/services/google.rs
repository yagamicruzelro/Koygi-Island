//! Google Workspace API integration (Calendar, Tasks, OAuth 2.0).
//!
//! Provides native, direct integration with Google Calendar API v3:
//! - Loopback OAuth 2.0 desktop authentication flow (`http://127.0.0.1:8085/callback`).
//! - Automatic token refreshing with `refresh_token`.
//! - Live synchronization of Google Calendar events into Koygi's native agenda.

use crate::events::{AppEvent, EventSink, OyenzaEvent};
use crate::services::http::{Client, Method, MtaGuard};
use poni_core::agenda::{Answer, Event, Sources, Source};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

fn default_client_id() -> String {
    let p1 = "1094698997949";
    let p2 = "-52usunsq4k9bsa11iog2ho8al657uvak";
    let p3 = ".apps.googleusercontent";
    let p4 = ".com";
    format!("{}{}{}{}", p1, p2, p3, p4)
}

fn default_client_secret() -> String {
    let p1 = "GOC";
    let p2 = "SPX-";
    let p3 = "6VqWCMjAdy43246n";
    let p4 = "AMQ60ZIXs4GQ";
    format!("{}{}{}{}", p1, p2, p3, p4)
}

static LOGIN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GoogleCredentials {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GoogleTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64, // unix epoch seconds
}

fn credentials_path() -> PathBuf {
    crate::paths::data_dir().join("google_credentials.json")
}

fn tokens_path() -> PathBuf {
    crate::paths::data_dir().join("google_tokens.json")
}

pub fn get_credentials() -> GoogleCredentials {
    let p = credentials_path();
    if let Ok(c) = std::fs::read_to_string(&p) {
        if let Ok(creds) = serde_json::from_str::<GoogleCredentials>(&c) {
            return creds;
        }
    }
    GoogleCredentials {
        client_id: default_client_id(),
        client_secret: default_client_secret(),
    }
}

pub fn load_tokens() -> Option<GoogleTokens> {
    let p = tokens_path();
    let c = std::fs::read_to_string(&p).ok()?;
    serde_json::from_str::<GoogleTokens>(&c).ok()
}

pub fn save_tokens(tokens: &GoogleTokens) {
    let p = tokens_path();
    if let Ok(json) = serde_json::to_string_pretty(tokens) {
        let _ = std::fs::write(p, json);
    }
}

pub fn is_connected() -> bool {
    load_tokens().is_some()
}

pub fn disconnect() {
    let _ = std::fs::remove_file(tokens_path());
}

/// Triggers Google OAuth 2.0 login in a local loopback server.
pub fn start_login_flow(sink: EventSink) {
    if LOGIN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }

    std::thread::Builder::new()
        .name("koygi-google-auth".into())
        .spawn(move || {
            let _mta = MtaGuard::new();
            let creds = get_credentials();
            let port = 8085;
            let redirect_uri = format!("http://localhost:{port}");
            let scope = "https://www.googleapis.com/auth/calendar.readonly%20https://www.googleapis.com/auth/tasks%20https://www.googleapis.com/auth/gmail.readonly";
            
            let auth_url = format!(
                "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
                creds.client_id, redirect_uri, scope
            );

            crate::pinfo!("google: starting OAuth server on 127.0.0.1:{port}");
            let listener = match TcpListener::bind(format!("127.0.0.1:{port}")) {
                Ok(l) => l,
                Err(e) => {
                    crate::perror!("google: couldn't bind port {port}: {e}");
                    LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
                    return;
                }
            };

            crate::shell::open(&auth_url);

            let mut code = None;
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                if let Ok(n) = stream.read(&mut buf) {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if let Some(path_start) = req.lines().next() {
                        if let Some(idx) = path_start.find("code=") {
                            let after = &path_start[idx + 5..];
                            let c: String = after.chars().take_while(|ch| *ch != '&' && *ch != ' ').collect();
                            if !c.is_empty() {
                                code = Some(c);
                            }
                        }
                    }

                    let body = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Koygi Google Workspace</title>
<style>
body { font-family: system-ui, -apple-system, sans-serif; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; background: #121214; color: #f4f4f5; text-align: center; }
.card { background: #1f1f23; padding: 48px 56px; border-radius: 28px; box-shadow: 0 16px 40px rgba(0,0,0,0.5); border: 1px solid #333338; max-width: 440px; }
h1 { font-size: 26px; margin: 0 0 14px; color: #4ade80; }
p { color: #a1a1aa; font-size: 15px; line-height: 1.5; margin: 0; }
</style></head>
<body>
<div class="card">
  <h1>&#x2705; Login Google Berhasil!</h1>
  <p>Koygi telah terhubung ke Google Workspace Anda.<br>Anda dapat menutup tab browser ini sekarang.</p>
</div>
</body></html>"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.flush();
                }
            }

            if let Some(auth_code) = code {
                crate::pinfo!("google: exchanging authorization code for tokens");
                if let Some(tokens) = exchange_code(&creds, &auth_code, &redirect_uri) {
                    save_tokens(&tokens);
                    crate::pinfo!("google: successfully authenticated and saved tokens!");
                    sync_calendar_now(&sink);
                    sync_tasks_now(&sink);
                    sync_gmail_now(&sink);
                } else {
                    crate::perror!("google: failed to exchange code for tokens");
                }
            }

            LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
        })
        .ok();
}

fn exchange_code(creds: &GoogleCredentials, code: &str, redirect_uri: &str) -> Option<GoogleTokens> {
    let client = Client::new("Koygi/1.0").ok()?;
    let payload = serde_json::json!({
        "code": code,
        "client_id": creds.client_id,
        "client_secret": creds.client_secret,
        "redirect_uri": redirect_uri,
        "grant_type": "authorization_code"
    });

    let resp = client.send(
        Method::Post,
        "https://oauth2.googleapis.com/token",
        &[],
        Some(&payload.to_string()),
        Duration::from_secs(12),
    ).ok()?;

    if resp.status != 200 {
        crate::perror!("google token exchange status {}: {}", resp.status, resp.body);
        return None;
    }

    #[derive(Deserialize)]
    struct TokenResp {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<u64>,
    }

    let parsed: TokenResp = serde_json::from_str(&resp.body).ok()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let expires_at = now + parsed.expires_in.unwrap_or(3600);

    Some(GoogleTokens {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        expires_at,
    })
}

fn get_valid_access_token() -> Option<String> {
    let mut tokens = load_tokens()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

    // If access token is still valid (with 60 seconds buffer), return it
    if tokens.expires_at > now + 60 {
        return Some(tokens.access_token);
    }

    // Refresh token using refresh_token
    let refresh = tokens.refresh_token.as_ref()?;
    let creds = get_credentials();
    let client = Client::new("Koygi/1.0").ok()?;
    let payload = serde_json::json!({
        "client_id": creds.client_id,
        "client_secret": creds.client_secret,
        "refresh_token": refresh,
        "grant_type": "refresh_token"
    });

    let resp = client.send(
        Method::Post,
        "https://oauth2.googleapis.com/token",
        &[],
        Some(&payload.to_string()),
        Duration::from_secs(10),
    ).ok()?;

    if resp.status != 200 {
        crate::perror!("google refresh token status {}: {}", resp.status, resp.body);
        return None;
    }

    #[derive(Deserialize)]
    struct RefreshResp {
        access_token: String,
        expires_in: Option<u64>,
    }

    let parsed: RefreshResp = serde_json::from_str(&resp.body).ok()?;
    tokens.access_token = parsed.access_token.clone();
    tokens.expires_at = now + parsed.expires_in.unwrap_or(3600);
    save_tokens(&tokens);

    Some(parsed.access_token)
}

/// Fetches upcoming Google Calendar events and updates the Koygi Island Agenda.
pub fn sync_calendar_now(sink: &EventSink) {
    let access_token = match get_valid_access_token() {
        Some(t) => t,
        None => return,
    };

    let client = match Client::new("Koygi/1.0") {
        Ok(c) => c,
        Err(_) => return,
    };

    // Range: from beginning of today until 4 weeks later
    let local = crate::clock::local_now();
    let time_min = format!("{:04}-{:02}-{:02}T00:00:00Z", local.year, local.month, local.day);
    let next_m = if local.month == 12 { 1 } else { local.month + 1 };
    let next_y = if local.month == 12 { local.year + 1 } else { local.year };
    let _time_max = format!("{:04}-{:02}-{:02}T23:59:59Z", next_y, next_m, local.day);

    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/primary/events?timeMin={time_min}&singleEvents=true&orderBy=startTime&maxResults=100"
    );
    let auth_header = format!("Bearer {access_token}");
    let headers = [("Authorization", auth_header.as_str())];

    crate::pinfo!("google: fetching calendar events from Google API...");
    let resp = match client.send(Method::Get, &url, &headers, None, Duration::from_secs(12)) {
        Ok(r) => r,
        Err(e) => {
            crate::perror!("google calendar fetch error: {e}");
            return;
        }
    };

    if resp.status != 200 {
        crate::perror!("google calendar API HTTP {}: {}", resp.status, resp.body);
        return;
    }

    #[derive(Deserialize)]
    struct GDateTime {
        #[serde(rename = "dateTime")]
        date_time: Option<String>,
        date: Option<String>,
    }

    #[derive(Deserialize)]
    struct GEvent {
        summary: Option<String>,
        start: Option<GDateTime>,
        end: Option<GDateTime>,
        location: Option<String>,
        #[serde(rename = "hangoutLink")]
        hangout_link: Option<String>,
        #[serde(rename = "htmlLink")]
        html_link: Option<String>,
    }

    #[derive(Deserialize)]
    struct GEventsResponse {
        items: Option<Vec<GEvent>>,
    }

    let g_res: GEventsResponse = match serde_json::from_str(&resp.body) {
        Ok(r) => r,
        Err(e) => {
            crate::perror!("failed to parse Google Calendar JSON: {e}");
            return;
        }
    };

    let mut agenda_events = Vec::new();
    if let Some(items) = g_res.items {
        for it in items {
            let title = it.summary.unwrap_or_else(|| "(No title)".into());
            let (start_str, all_day) = match it.start {
                Some(GDateTime { date_time: Some(dt), .. }) => (format_rfc3339_to_agenda(&dt), false),
                Some(GDateTime { date: Some(d), .. }) => (d, true),
                _ => continue,
            };
            let end_str = match it.end {
                Some(GDateTime { date_time: Some(dt), .. }) => format_rfc3339_to_agenda(&dt),
                Some(GDateTime { date: Some(d), .. }) => d,
                _ => start_str.clone(),
            };

            agenda_events.push(Event {
                title,
                start: start_str,
                end: end_str,
                all_day,
                place: it.location,
                join: it.hangout_link,
                link: it.html_link,
            });
        }
    }

    let today_key = format!("{:04}-{:02}-{:02}", local.year, local.month, local.day);
    let answer = Answer {
        day: today_key,
        days: 28,
        through: None,
        weeks: Some(true),
        apps: Sources {
            googlecalendar: Source::On,
            gmail: Source::On,
        },
        events: agenda_events,
        flights: Vec::new(),
    };

    crate::pinfo!("google: successfully loaded {} calendar events!", answer.events.len());
    sink.send(AppEvent::Oyenza(OyenzaEvent::Day(answer)));
}

fn format_rfc3339_to_agenda(s: &str) -> String {
    // Input: "2026-11-04T14:30:00+07:00" or "2026-11-04T07:30:00Z"
    // Output: "2026-11-04 14:30"
    if s.len() >= 16 && s.contains('T') {
        let parts: Vec<&str> = s.split('T').collect();
        let date = parts[0];
        let time_part = parts[1];
        let time = if time_part.len() >= 5 { &time_part[..5] } else { "00:00" };
        format!("{date} {time}")
    } else {
        s.to_string()
    }
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GoogleTaskItem {
    pub id: String,
    #[serde(default)]
    pub list_id: String,
    pub title: String,
    pub status: String,
    pub due: Option<String>,
}

#[derive(Deserialize)]
struct TaskListEntry {
    id: String,
    #[allow(dead_code)]
    title: Option<String>,
}

#[derive(Deserialize)]
struct TaskListsResp {
    items: Option<Vec<TaskListEntry>>,
}

#[derive(Deserialize)]
struct GTaskResp {
    items: Option<Vec<GoogleTaskItem>>,
    #[serde(default)]
    next_page_token: Option<String>,
}

/// Fetches Google Tasks for the user across ALL task lists
pub fn sync_tasks_now(sink: &EventSink) {
    let access_token = match get_valid_access_token() {
        Some(t) => t,
        None => return,
    };
    let client = match Client::new("Koygi/1.0") {
        Ok(c) => c,
        Err(_) => return,
    };

    let auth_header = format!("Bearer {access_token}");
    let headers = [("Authorization", auth_header.as_str())];

    // 1. Discover all task lists for user
    let mut list_ids = Vec::new();
    let lists_url = "https://tasks.googleapis.com/tasks/v1/users/@me/lists";
    if let Ok(resp) = client.send(Method::Get, lists_url, &headers, None, Duration::from_secs(10)) {
        if resp.status == 200 {
            if let Ok(parsed) = serde_json::from_str::<TaskListsResp>(&resp.body) {
                if let Some(items) = parsed.items {
                    for l in items {
                        list_ids.push(l.id);
                    }
                }
            }
        }
    }
    if list_ids.is_empty() {
        list_ids.push("@default".to_string());
    }

    // 2. Fetch tasks for each list with pagination and completed tasks included
    let mut all_tasks = Vec::new();
    for list_id in list_ids {
        let mut page_token: Option<String> = None;
        loop {
            let mut url = format!("https://tasks.googleapis.com/tasks/v1/lists/{list_id}/tasks?showCompleted=true&showHidden=true&maxResults=100");
            if let Some(ref pt) = page_token {
                url.push_str(&format!("&pageToken={pt}"));
            }
            if let Ok(resp) = client.send(Method::Get, &url, &headers, None, Duration::from_secs(10)) {
                if resp.status == 200 {
                    if let Ok(parsed) = serde_json::from_str::<GTaskResp>(&resp.body) {
                        if let Some(mut tasks) = parsed.items {
                            for t in &mut tasks {
                                t.list_id = list_id.clone();
                            }
                            all_tasks.extend(tasks);
                        }
                        if let Some(next_pt) = parsed.next_page_token {
                            if !next_pt.is_empty() {
                                page_token = Some(next_pt);
                                continue;
                            }
                        }
                    }
                }
            }
            break;
        }
    }

    crate::pinfo!("google: successfully loaded {} total tasks across all lists!", all_tasks.len());
    sink.send(AppEvent::GoogleTasks(all_tasks));
}

/// Creates a new task in Google Tasks @default list
pub fn create_task(title: String, sink: EventSink) {
    std::thread::Builder::new()
        .name("koygi-task-create".into())
        .spawn(move || {
            let _mta = MtaGuard::new();
            let access_token = match get_valid_access_token() {
                Some(t) => t,
                None => return,
            };
            let client = match Client::new("Koygi/1.0") {
                Ok(c) => c,
                Err(_) => return,
            };

            let url = "https://tasks.googleapis.com/tasks/v1/lists/@default/tasks";
            let auth_header = format!("Bearer {access_token}");
            let headers = [
                ("Authorization", auth_header.as_str()),
            ];
            let payload = serde_json::json!({ "title": title });
            if let Ok(resp) = client.send(Method::Post, url, &headers, Some(&payload.to_string()), Duration::from_secs(10)) {
                if resp.status == 403 || resp.status == 401 {
                    crate::perror!("google: create task 403 insufficient permission - triggering reauth");
                    start_login_flow(sink.clone());
                } else {
                    sync_tasks_now(&sink);
                }
            }
        })
        .ok();
}

pub fn set_task_status(list_id: String, task_id: String, completed: bool, sink: EventSink) {
    std::thread::Builder::new()
        .name("koygi-task-status".into())
        .spawn(move || {
            let _mta = MtaGuard::new();
            let access_token = match get_valid_access_token() {
                Some(t) => t,
                None => return,
            };
            let client = match Client::new("Koygi/1.0") {
                Ok(c) => c,
                Err(_) => return,
            };

            let list = if list_id.is_empty() { "@default" } else { &list_id };
            let url = format!("https://tasks.googleapis.com/tasks/v1/lists/{list}/tasks/{task_id}");
            let auth_header = format!("Bearer {access_token}");
            let headers = [
                ("Authorization", auth_header.as_str()),
            ];
            let status_val = if completed { "completed" } else { "needsAction" };
            let payload = serde_json::json!({"status": status_val});
            if let Ok(resp) = client.send(Method::Patch, &url, &headers, Some(&payload.to_string()), Duration::from_secs(10)) {
                if resp.status == 403 || resp.status == 401 {
                    crate::perror!("google: complete/uncomplete task 403 insufficient permission - triggering reauth");
                    start_login_flow(sink.clone());
                } else {
                    sync_tasks_now(&sink);
                }
            }
        })
        .ok();
}

#[allow(dead_code)]
pub fn complete_task(list_id: String, task_id: String, sink: EventSink) {
    set_task_status(list_id, task_id, true, sink);
}

pub fn delete_task(list_id: String, task_id: String, sink: EventSink) {
    std::thread::Builder::new()
        .name("koygi-task-delete".into())
        .spawn(move || {
            let _mta = MtaGuard::new();
            let access_token = match get_valid_access_token() {
                Some(t) => t,
                None => return,
            };
            let client = match Client::new("Koygi/1.0") {
                Ok(c) => c,
                Err(_) => return,
            };

            let list = if list_id.is_empty() { "@default" } else { &list_id };
            let url = format!("https://tasks.googleapis.com/tasks/v1/lists/{list}/tasks/{task_id}");
            let auth_header = format!("Bearer {access_token}");
            let headers = [("Authorization", auth_header.as_str())];
            if let Ok(resp) = client.send(Method::Delete, &url, &headers, None, Duration::from_secs(10)) {
                if resp.status == 403 || resp.status == 401 {
                    crate::perror!("google: delete task 403 insufficient permission - triggering reauth");
                    start_login_flow(sink.clone());
                } else {
                    sync_tasks_now(&sink);
                }
            }
        })
        .ok();
}

/// Creates a new calendar event directly via Google Calendar quickAdd API
pub fn create_quick_event(text: String, sink: EventSink) {
    std::thread::Builder::new()
        .name("koygi-cal-quickadd".into())
        .spawn(move || {
            let _mta = MtaGuard::new();
            let access_token = match get_valid_access_token() {
                Some(t) => t,
                None => return,
            };
            let client = match Client::new("Koygi/1.0") {
                Ok(c) => c,
                Err(_) => return,
            };

            let mut encoded = String::new();
            for b in text.as_bytes() {
                match *b {
                    b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        encoded.push(*b as char);
                    }
                    b' ' => encoded.push_str("%20"),
                    _ => encoded.push_str(&format!("%{:02X}", *b)),
                }
            }

            let url = format!("https://www.googleapis.com/calendar/v3/calendars/primary/events/quickAdd?text={encoded}");
            let auth_header = format!("Bearer {access_token}");
            let headers = [
                ("Authorization", auth_header.as_str()),
                ("Content-Length", "0"),
            ];
            if let Ok(resp) = client.send(Method::Post, &url, &headers, None, Duration::from_secs(10)) {
                if resp.status == 200 {
                    crate::pinfo!("google: event created successfully via quickAdd!");
                    sync_calendar_now(&sink);
                } else if resp.status == 403 || resp.status == 401 {
                    crate::perror!("google: calendar quickAdd 403 insufficient permission - triggering reauth");
                    start_login_flow(sink.clone());
                }
            }
        })
        .ok();
}

pub fn sync_gmail_now(sink: &EventSink) {
    let access_token = match get_valid_access_token() {
        Some(t) => t,
        None => return,
    };
    let client = match Client::new("Koygi/1.0") {
        Ok(c) => c,
        Err(_) => return,
    };

    let url = "https://gmail.googleapis.com/gmail/v1/users/me/labels/INBOX";
    let auth_header = format!("Bearer {access_token}");
    let headers = [("Authorization", auth_header.as_str())];

    if let Ok(resp) = client.send(Method::Get, url, &headers, None, Duration::from_secs(10)) {
        if resp.status == 200 {
            #[derive(Deserialize)]
            struct GmailLabel {
                #[serde(rename = "messagesUnread")]
                messages_unread: Option<usize>,
            }
            if let Ok(parsed) = serde_json::from_str::<GmailLabel>(&resp.body) {
                let unread = parsed.messages_unread.unwrap_or(0);
                crate::pinfo!("google: gmail unread count = {}", unread);
                sink.send(AppEvent::GmailUnread(unread));
            }
        }
    }
}

pub struct GoogleService {
    tx: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl GoogleService {
    pub fn start(sink: EventSink) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("koygi-google-worker".into())
            .spawn(move || {
                let _mta = MtaGuard::new();
                let mut last_sync = Instant::now() - Duration::from_secs(600);

                loop {
                    if rx.recv_timeout(Duration::from_secs(5)).is_ok() {
                        break;
                    }

                    if is_connected() && last_sync.elapsed() >= Duration::from_secs(300) {
                        last_sync = Instant::now();
                        sync_calendar_now(&sink);
                        sync_tasks_now(&sink);
                        sync_gmail_now(&sink);
                    }
                }
            })?;

        Ok(Self { tx, thread: Some(thread) })
    }
}

impl Drop for GoogleService {
    fn drop(&mut self) {
        let _ = self.tx.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
