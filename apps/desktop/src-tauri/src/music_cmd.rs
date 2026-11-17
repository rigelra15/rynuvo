//! Tauri commands for listen-together music.
use std::sync::Arc;

use rynuvo_proto::{Id, MusicCmd, WsClientMsg};
use tauri::State;

use crate::{
    commands::Core,
    music::{
        ctl::{MusicStatus, Output},
        lyrics::{self, Lyrics},
        webm::Song,
        ytm,
    },
    prefs::PrefsStore,
    voice::ctl::VoiceCtl,
};

pub const MUSIC_EVENT: &str = "rynuvo://music";

/// The HTTP client music uses (its own: YouTube is slow sometimes, the app's API shouldn't wait).
pub struct MusicHttp(pub reqwest::Client);

/// Playback goes to the voice engine.
pub struct VoiceOut(pub VoiceCtl);

impl Output for VoiceOut {
    fn set(&self, entry: u64, song: Arc<Song>, at_ms: u32, paused: bool) {
        self.0.music_set(entry, song, at_ms, paused);
    }
    fn clear(&self) {
        self.0.music_clear();
    }
    fn channel(&self) -> Option<Id> {
        self.0.channel()
    }
}

/// Fetch and decode-ready a song: resolve its stream, download it, demux it.
pub fn loader(http: reqwest::Client) -> crate::music::ctl::Loader {
    Arc::new(move |video_id: String| {
        let http = http.clone();
        Box::pin(async move {
            let s = ytm::resolve(&http, &video_id).await?;
            let data = ytm::download(&http, &s).await?;
            tauri::async_runtime::spawn_blocking(move || crate::music::webm::demux(&data)).await.map_err(|e| e.to_string())?
        })
    })
}

/// `!p query`: search YouTube Music and queue the top song (or tell the channel nothing was found).
#[tauri::command]
pub async fn music_play(core: State<'_, Core>, http: State<'_, MusicHttp>, query: String, text_channel_id: Option<Id>) -> Result<(), String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Err("Type a song after the command, like !p LANY Stuck".into());
    }
    let cmd = match ytm::search(&http.0, &query).await?.into_iter().next() {
        Some(entry) => MusicCmd::Add { entry },
        None => MusicCmd::NotFound { query },
    };
    core.voice.send_ws(WsClientMsg::Music { cmd, text_channel_id });
    Ok(())
}

/// Songs, then music videos, matching `query` on YouTube Music, best first (for the song picker).
/// Videos are a bonus: if their search fails, the songs still come back.
#[tauri::command]
pub async fn music_search(http: State<'_, MusicHttp>, query: String) -> Result<Vec<rynuvo_proto::MusicEntryReq>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(vec![]);
    }
    let (songs, videos) = tokio::join!(ytm::search(&http.0, query), ytm::search_videos(&http.0, query));
    let mut found = songs?;
    found.truncate(6);
    found.extend(videos.unwrap_or_default().into_iter().take(5));
    Ok(found)
}

/// Where to stream a music video's picture from (the sound comes through the music player).
#[tauri::command]
pub async fn music_video_url(http: State<'_, MusicHttp>, video_id: String) -> Result<String, String> {
    ytm::resolve_video(&http.0, &video_id).await.inspect_err(|e| tracing::warn!("music video {video_id}: {e}"))
}

/// Any other music command (skip, pause, seek, queue, …).
#[tauri::command]
pub fn music_cmd(core: State<'_, Core>, cmd: MusicCmd, text_channel_id: Option<Id>) {
    core.voice.send_ws(WsClientMsg::Music { cmd, text_channel_id });
}

#[tauri::command]
pub async fn music_lyrics(http: State<'_, MusicHttp>, video_id: String, title: String, artist: String, duration_ms: u32, source: Option<String>) -> Result<Option<Lyrics>, String> {
    Ok(lyrics::fetch(&http.0, source.as_deref().unwrap_or("auto"), &video_id, &title, &artist, duration_ms).await)
}

/// English translation and romanization for each lyric line (same order).
#[tauri::command]
pub async fn music_annotate(http: State<'_, MusicHttp>, lines: Vec<String>) -> Result<Vec<crate::music::translate::Annotation>, String> {
    Ok(crate::music::translate::annotate(&http.0, &lines).await)
}

#[tauri::command]
pub fn music_status(core: State<'_, Core>, video_id: String) -> Option<MusicStatus> {
    core.music.status_of(&video_id)
}

#[tauri::command]
pub fn music_set_volume(core: State<'_, Core>, prefs: State<'_, PrefsStore>, volume: f32) {
    core.voice.set_music_volume(volume);
    prefs.update(|p| p.music_volume = volume.clamp(0.0, 1.0));
}
