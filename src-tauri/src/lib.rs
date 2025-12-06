use chrono::{DateTime, Utc};
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;
use tauri::State;
use uuid::Uuid;

// Database state
pub struct DbState(pub Mutex<Connection>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub video_id: String,
    pub title: String,
    pub channel: String,
    pub thumbnail_url: String,
    pub source_url: String,
    pub start_time: String,
    pub end_time: String,
    pub file_path: String,
    pub file_size: i64,
    pub created_at: String,
    pub transcript: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClipResult {
    success: bool,
    message: String,
    file_path: Option<String>,
    file_size: Option<String>,
    duration_secs: u64,
    clip: Option<Clip>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct YouTubeMetadata {
    video_id: String,
    title: String,
    channel: String,
    thumbnail_url: String,
}

const PLACEHOLDER_MAX_SIZE: u64 = 5000;

fn get_clips_dir() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let clips_dir = base.join("Clippa").join("clips");
    std::fs::create_dir_all(&clips_dir).ok();
    clips_dir
}

fn get_db_path() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let data_dir = base.join("Clippa");
    std::fs::create_dir_all(&data_dir).ok();
    data_dir.join("clips.db")
}

fn init_db(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS clips (
            id TEXT PRIMARY KEY,
            video_id TEXT NOT NULL,
            title TEXT NOT NULL,
            channel TEXT NOT NULL,
            thumbnail_url TEXT NOT NULL,
            source_url TEXT NOT NULL,
            start_time TEXT NOT NULL,
            end_time TEXT NOT NULL,
            file_path TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            transcript TEXT
        )",
        [],
    )?;
    Ok(())
}

fn extract_video_id(url: &str) -> Option<String> {
    // Match various YouTube URL formats
    let patterns = [
        r"(?:youtube\.com/watch\?v=|youtu\.be/|youtube\.com/embed/|youtube\.com/v/)([a-zA-Z0-9_-]{11})",
        r"youtube\.com/shorts/([a-zA-Z0-9_-]{11})",
    ];

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(url) {
                if let Some(id) = caps.get(1) {
                    return Some(id.as_str().to_string());
                }
            }
        }
    }
    None
}

fn resolve_best_thumbnail(video_id: &str) -> String {
    let maxres_url = format!("https://i.ytimg.com/vi/{}/maxresdefault.jpg", video_id);
    let hq_url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id);

    // Try to check if maxres exists using HEAD request
    if let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        if let Ok(response) = client.head(&maxres_url).send() {
            if let Some(content_length) = response.headers().get("content-length") {
                if let Ok(size_str) = content_length.to_str() {
                    if let Ok(size) = size_str.parse::<u64>() {
                        if size > PLACEHOLDER_MAX_SIZE {
                            return maxres_url;
                        }
                    }
                }
            }
        }
    }

    hq_url
}

fn fetch_youtube_metadata(url: &str) -> Result<YouTubeMetadata, String> {
    let video_id = extract_video_id(url).ok_or("Could not extract video ID from URL")?;

    // Use yt-dlp to get metadata (most reliable)
    let output = Command::new("yt-dlp")
        .args([
            "--dump-json",
            "--no-download",
            "--no-playlist",
            url,
        ])
        .output()
        .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;

    if !output.status.success() {
        // Fallback to basic metadata
        return Ok(YouTubeMetadata {
            video_id: video_id.clone(),
            title: "Unknown Title".to_string(),
            channel: "Unknown Channel".to_string(),
            thumbnail_url: resolve_best_thumbnail(&video_id),
        });
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse yt-dlp JSON: {}", e))?;

    let title = json["title"]
        .as_str()
        .unwrap_or("Unknown Title")
        .to_string();
    let channel = json["uploader"]
        .as_str()
        .or_else(|| json["channel"].as_str())
        .unwrap_or("Unknown Channel")
        .to_string();

    // Try to get thumbnail from yt-dlp response, or resolve best
    let thumbnail_url = json["thumbnail"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| resolve_best_thumbnail(&video_id));

    Ok(YouTubeMetadata {
        video_id,
        title,
        channel,
        thumbnail_url,
    })
}

#[tauri::command]
async fn download_clip(
    url: String,
    start: String,
    end: String,
    db: State<'_, DbState>,
) -> Result<ClipResult, String> {
    let start_time = Instant::now();

    // Fetch YouTube metadata first
    let metadata = fetch_youtube_metadata(&url)?;

    // Generate unique filename
    let clip_id = Uuid::new_v4().to_string();
    let clips_dir = get_clips_dir();
    let safe_title: String = metadata
        .title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .take(50)
        .collect();
    let filename = format!("{}_{}.mp4", safe_title, &clip_id[..8]);
    let output_path = clips_dir.join(&filename);
    let output_path_str = output_path.to_string_lossy().to_string();

    // Build yt-dlp command
    let output = Command::new("yt-dlp")
        .args([
            "--download-sections",
            &format!("*{}-{}", start, end),
            "--force-keyframes-at-cuts",
            "-f",
            "bestvideo[height<=1080]+bestaudio/best",
            "--merge-output-format",
            "mp4",
            "--postprocessor-args",
            "ffmpeg:-c:v h264_videotoolbox",
            "-o",
            &output_path_str,
            &url,
        ])
        .output()
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    let duration_secs = start_time.elapsed().as_secs();

    if output.status.success() {
        let file_size = std::fs::metadata(&output_path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);

        let file_size_str = if file_size > 1024 * 1024 {
            format!("{:.1} MB", file_size as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1} KB", file_size as f64 / 1024.0)
        };

        let created_at: DateTime<Utc> = Utc::now();
        let created_at_str = created_at.to_rfc3339();

        // Save to database
        let clip = Clip {
            id: clip_id,
            video_id: metadata.video_id,
            title: metadata.title,
            channel: metadata.channel,
            thumbnail_url: metadata.thumbnail_url,
            source_url: url,
            start_time: start,
            end_time: end,
            file_path: output_path_str.clone(),
            file_size,
            created_at: created_at_str,
            transcript: None,
        };

        {
            let conn = db.0.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO clips (id, video_id, title, channel, thumbnail_url, source_url, start_time, end_time, file_path, file_size, created_at, transcript)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    clip.id,
                    clip.video_id,
                    clip.title,
                    clip.channel,
                    clip.thumbnail_url,
                    clip.source_url,
                    clip.start_time,
                    clip.end_time,
                    clip.file_path,
                    clip.file_size,
                    clip.created_at,
                    clip.transcript,
                ],
            )
            .map_err(|e| format!("Database error: {}", e))?;
        }

        Ok(ClipResult {
            success: true,
            message: "Clip downloaded and saved".to_string(),
            file_path: Some(output_path_str),
            file_size: Some(file_size_str),
            duration_secs,
            clip: Some(clip),
        })
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("yt-dlp error: {}", stderr))
    }
}

#[tauri::command]
fn get_all_clips(db: State<'_, DbState>) -> Result<Vec<Clip>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT id, video_id, title, channel, thumbnail_url, source_url, start_time, end_time, file_path, file_size, created_at, transcript FROM clips ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;

    let clips = stmt
        .query_map([], |row| {
            Ok(Clip {
                id: row.get(0)?,
                video_id: row.get(1)?,
                title: row.get(2)?,
                channel: row.get(3)?,
                thumbnail_url: row.get(4)?,
                source_url: row.get(5)?,
                start_time: row.get(6)?,
                end_time: row.get(7)?,
                file_path: row.get(8)?,
                file_size: row.get(9)?,
                created_at: row.get(10)?,
                transcript: row.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(clips)
}

#[tauri::command]
fn get_clip(id: String, db: State<'_, DbState>) -> Result<Option<Clip>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT id, video_id, title, channel, thumbnail_url, source_url, start_time, end_time, file_path, file_size, created_at, transcript FROM clips WHERE id = ?1")
        .map_err(|e| e.to_string())?;

    let clip = stmt
        .query_row(params![id], |row| {
            Ok(Clip {
                id: row.get(0)?,
                video_id: row.get(1)?,
                title: row.get(2)?,
                channel: row.get(3)?,
                thumbnail_url: row.get(4)?,
                source_url: row.get(5)?,
                start_time: row.get(6)?,
                end_time: row.get(7)?,
                file_path: row.get(8)?,
                file_size: row.get(9)?,
                created_at: row.get(10)?,
                transcript: row.get(11)?,
            })
        })
        .ok();

    Ok(clip)
}

#[tauri::command]
fn delete_clip(id: String, db: State<'_, DbState>) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;

    // Get file path first
    let file_path: Option<String> = conn
        .query_row(
            "SELECT file_path FROM clips WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .ok();

    // Delete file if exists
    if let Some(path) = file_path {
        std::fs::remove_file(&path).ok();
    }

    // Delete from database
    conn.execute("DELETE FROM clips WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn update_clip_transcript(id: String, transcript: String, db: State<'_, DbState>) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE clips SET transcript = ?1 WHERE id = ?2",
        params![transcript, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_downloads_dir() -> Result<String, String> {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not find downloads directory".to_string())
}

#[tauri::command]
fn get_clips_directory() -> String {
    get_clips_dir().to_string_lossy().to_string()
}

#[tauri::command]
fn open_file_location(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn play_clip(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscribeJobResponse {
    job_id: Option<String>,
    status: String,
    transcript: Option<String>,
    error: Option<String>,
}

#[tauri::command]
async fn transcribe_clip(file_path: String) -> Result<TranscribeJobResponse, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .post("http://localhost:8765/transcribe")
        .json(&serde_json::json!({
            "file_path": file_path,
            "async": true
        }))
        .send()
        .map_err(|e| format!("Whisper service not running: {}", e))?;

    let result: TranscribeJobResponse = response
        .json()
        .map_err(|e| format!("Invalid response: {}", e))?;

    Ok(result)
}

#[tauri::command]
async fn get_transcription_status(job_id: String) -> Result<TranscribeJobResponse, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(format!("http://localhost:8765/jobs/{}", job_id))
        .send()
        .map_err(|e| format!("Whisper service error: {}", e))?;

    #[derive(Deserialize)]
    struct JobStatus {
        status: String,
        result: Option<TranscriptResult>,
        error: Option<String>,
    }

    #[derive(Deserialize)]
    struct TranscriptResult {
        transcript: String,
    }

    let status: JobStatus = response
        .json()
        .map_err(|e| format!("Invalid response: {}", e))?;

    Ok(TranscribeJobResponse {
        job_id: Some(job_id),
        status: status.status.clone(),
        transcript: status.result.map(|r| r.transcript),
        error: status.error,
    })
}

#[tauri::command]
fn check_whisper_service() -> bool {
    if let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        client
            .get("http://localhost:8765/health")
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    } else {
        false
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize database
    let db_path = get_db_path();
    let conn = Connection::open(&db_path).expect("Failed to open database");
    init_db(&conn).expect("Failed to initialize database");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .manage(DbState(Mutex::new(conn)))
        .invoke_handler(tauri::generate_handler![
            download_clip,
            get_all_clips,
            get_clip,
            delete_clip,
            update_clip_transcript,
            get_downloads_dir,
            get_clips_directory,
            open_file_location,
            play_clip,
            transcribe_clip,
            get_transcription_status,
            check_whisper_service
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
