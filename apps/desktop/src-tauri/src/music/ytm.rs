//! YouTube Music through its web API (the way the owner's musicfy does it): song search with the
//! WEB_REMIX client, and audio stream URLs with the VISIONOS client, which returns plain URLs.
use rynuvo_proto::MusicEntryReq;
use serde_json::{Value, json};

const BASE: &str = "https://music.youtube.com";
const REMIX_VERSION: &str = "1.20260707.12.00";
/// The search filter for "Songs".
const SONG_FILTER: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";

/// The audio stream chosen for a song.
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub url: String,
    pub content_length: Option<u64>,
}

/// "3:04", "3.04" or "1:02:03" → ms.
fn clock_ms(s: &str) -> Option<u32> {
    let mut total: u32 = 0;
    for part in s.trim().split([':', '.']) {
        total = total.checked_mul(60)?.checked_add(part.parse().ok()?)?;
    }
    s.contains([':', '.']).then_some(total * 1000)
}

fn text(v: &Value) -> &str {
    v.get("text").and_then(Value::as_str).unwrap_or("")
}

fn row(r: &Value) -> Option<MusicEntryReq> {
    let video_id = r.pointer("/playlistItemData/videoId")?.as_str()?.to_string();
    let col = |i: usize| r.pointer(&format!("/flexColumns/{i}/musicResponsiveListItemFlexColumnRenderer/text/runs")).and_then(Value::as_array);
    let title = text(col(0)?.first()?).to_string();
    // Metadata runs split into groups on " • ": artists, album, duration.
    let mut groups: Vec<Vec<&Value>> = vec![Vec::new()];
    for run in col(1)? {
        if text(run) == " • " {
            groups.push(Vec::new());
        } else {
            groups.last_mut().unwrap().push(run);
        }
    }
    let artist = groups[0].iter().step_by(2).map(|r| text(r)).collect::<Vec<_>>().join(", ");
    let album = groups.get(1).and_then(|g| g.iter().find(|r| r.pointer("/navigationEndpoint/browseEndpoint").is_some())).map(|r| text(r).to_string());
    let duration_ms = groups.last().and_then(|g| g.first()).and_then(|r| clock_ms(text(r)))?;
    let art_url = r
        .pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails")
        .and_then(Value::as_array)
        .and_then(|t| t.last())
        .and_then(|t| t.get("url"))
        .and_then(Value::as_str)
        .map(big_art)
        .unwrap_or_default();
    Some(MusicEntryReq { video_id, title, artist, album, art_url, duration_ms })
}

/// Thumbnails come as small squares (`=w120-h120-…`); ask for a large one for the backdrop.
fn big_art(url: &str) -> String {
    match url.rfind("=w") {
        Some(i) if url[i..].contains("-h") => format!("{}=w544-h544-l90-rj", &url[..i]),
        _ => url.to_string(),
    }
}

/// Songs in a search response, best first.
pub fn parse_search(v: &Value) -> Vec<MusicEntryReq> {
    let sections = v
        .pointer("/contents/tabbedSearchResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(Value::as_array);
    let Some(shelf) = sections.and_then(|s| s.iter().find_map(|c| c.get("musicShelfRenderer"))) else { return Vec::new() };
    shelf
        .get("contents")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().filter_map(|r| r.get("musicResponsiveListItemRenderer")).filter_map(row).collect())
        .unwrap_or_default()
}

/// The audio stream to play from a player response: Opus (WebM) only, highest bitrate.
pub fn pick_stream(v: &Value) -> Result<Stream, String> {
    let status = v.pointer("/playabilityStatus/status").and_then(Value::as_str).unwrap_or("");
    if status != "OK" {
        let why = v.pointer("/playabilityStatus/reason").and_then(Value::as_str).unwrap_or("not playable");
        return Err(format!("YouTube Music: {why}"));
    }
    let formats = v.pointer("/streamingData/adaptiveFormats").and_then(Value::as_array).cloned().unwrap_or_default();
    formats
        .iter()
        .filter(|f| f.get("mimeType").and_then(Value::as_str).is_some_and(|m| m.starts_with("audio/webm") && m.contains("opus")))
        .filter_map(|f| {
            let url = f.get("url")?.as_str()?;
            url.starts_with("https://").then_some(())?;
            let bitrate = f.get("bitrate").and_then(Value::as_u64).unwrap_or(0);
            let len = f.get("contentLength").and_then(|l| l.as_str().and_then(|s| s.parse().ok()).or(l.as_u64()));
            Some((bitrate, Stream { url: url.to_string(), content_length: len }))
        })
        .max_by_key(|(b, _)| *b)
        .map(|(_, s)| s)
        .ok_or_else(|| "No playable audio stream for this song".to_string())
}

/// The visitor id from `sw.js_data` (needed by the player endpoint).
pub fn parse_visitor(body: &str) -> Option<String> {
    let body = body.strip_prefix(")]}'").unwrap_or(body);
    let v: Value = serde_json::from_str(body.trim_start()).ok()?;
    fn find(v: &Value) -> Option<String> {
        match v {
            Value::String(s) if s.starts_with("Cgt") || s.starts_with("Cgs") => Some(s.clone()),
            Value::Array(a) => a.iter().find_map(find),
            _ => None,
        }
    }
    find(v.pointer("/0/2")?)
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("http client")
}

pub async fn search(http: &reqwest::Client, query: &str) -> Result<Vec<MusicEntryReq>, String> {
    let body = json!({
        "context": {"client": {"clientName": "WEB_REMIX", "clientVersion": REMIX_VERSION, "gl": "ID", "hl": "en"}},
        "query": query,
        "params": SONG_FILTER,
    });
    let v: Value = http
        .post(format!("{BASE}/youtubei/v1/search?prettyPrint=false"))
        .header("X-YouTube-Client-Name", "67")
        .header("X-YouTube-Client-Version", REMIX_VERSION)
        .header("X-Origin", BASE)
        .header("Origin", BASE)
        .header("X-Goog-Api-Format-Version", "1")
        .json(&body)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Couldn't reach YouTube Music ({e})"))?
        .json()
        .await
        .map_err(|e| format!("YouTube Music sent something unexpected ({e})"))?;
    Ok(parse_search(&v))
}

pub async fn resolve(http: &reqwest::Client, video_id: &str) -> Result<Stream, String> {
    let sw = http.get(format!("{BASE}/sw.js_data")).send().await.and_then(|r| r.error_for_status()).map_err(|e| format!("Couldn't reach YouTube Music ({e})"))?;
    let visitor = parse_visitor(&sw.text().await.map_err(|e| e.to_string())?).ok_or("YouTube Music changed its visitor data")?;
    let body = json!({
        "context": {"client": {
            "clientName": "VISIONOS", "clientVersion": "0.1", "osName": "visionOS", "osVersion": "1.3.21O771",
            "deviceMake": "Apple", "deviceModel": "RealityDevice14,1", "gl": "ID", "hl": "en", "visitorData": visitor,
        }},
        "videoId": video_id,
        "playlistId": null,
        "contentCheckOk": true,
        "racyCheckOk": true,
    });
    let v: Value = http
        .post(format!("{BASE}/youtubei/v1/player?prettyPrint=false"))
        .header("X-Goog-Visitor-Id", &visitor)
        .header("X-YouTube-Client-Name", "101")
        .header("X-YouTube-Client-Version", "0.1")
        .header("Origin", BASE)
        .json(&body)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Couldn't reach YouTube Music ({e})"))?
        .json()
        .await
        .map_err(|e| format!("YouTube Music sent something unexpected ({e})"))?;
    pick_stream(&v)
}

/// Download the whole audio stream (a few MB). A range request avoids YouTube's throttling.
pub async fn download(http: &reqwest::Client, s: &Stream) -> Result<Vec<u8>, String> {
    let mut req = http.get(&s.url).timeout(std::time::Duration::from_secs(90));
    if let Some(n) = s.content_length.filter(|n| *n > 0) {
        req = req.header("Range", format!("bytes=0-{}", n - 1));
    }
    let r = req.send().await.and_then(|r| r.error_for_status()).map_err(|e| format!("Couldn't download the song ({e})"))?;
    r.bytes().await.map(|b| b.to_vec()).map_err(|e| format!("Couldn't download the song ({e})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(t: &str) -> Value {
        json!({"text": t})
    }

    fn search_fixture() -> Value {
        let item = json!({"musicResponsiveListItemRenderer": {
            "playlistItemData": {"videoId": "EjvAa0RxDLo"},
            "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [
                {"url": "https://lh3.googleusercontent.com/abc=w60-h60-l90-rj"},
                {"url": "https://lh3.googleusercontent.com/abc=w120-h120-l90-rj"}]}}},
            "flexColumns": [
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [run("Stuck")]}}},
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [
                    run("LANY"), run(" & "), run("keshi"), run(" • "),
                    {"text": "Soft", "navigationEndpoint": {"browseEndpoint": {"browseId": "MPRE"}}},
                    run(" • "), run("3:04")]}}}]
        }});
        let broken = json!({"musicResponsiveListItemRenderer": {"flexColumns": []}});
        json!({"contents": {"tabbedSearchResultsRenderer": {"tabs": [{"tabRenderer": {"content": {"sectionListRenderer": {"contents": [
            {"itemSectionRenderer": {}},
            {"musicShelfRenderer": {"contents": [broken, item]}}
        ]}}}}]}}})
    }

    #[test]
    fn search_rows_become_entries() {
        let r = parse_search(&search_fixture());
        assert_eq!(r.len(), 1, "rows without a video are skipped");
        let e = &r[0];
        assert_eq!(e.video_id, "EjvAa0RxDLo");
        assert_eq!(e.title, "Stuck");
        assert_eq!(e.artist, "LANY, keshi");
        assert_eq!(e.album.as_deref(), Some("Soft"));
        assert_eq!(e.duration_ms, 184_000);
        assert_eq!(e.art_url, "https://lh3.googleusercontent.com/abc=w544-h544-l90-rj");
        assert!(parse_search(&json!({})).is_empty());
    }

    #[test]
    fn clock_formats() {
        assert_eq!(clock_ms("3:04"), Some(184_000));
        assert_eq!(clock_ms("3.04"), Some(184_000));
        assert_eq!(clock_ms("1:02:03"), Some(3_723_000));
        assert_eq!(clock_ms("Soft"), None);
        assert_eq!(clock_ms("2024"), None);
    }

    #[test]
    fn picks_the_best_opus_stream() {
        let v = json!({"playabilityStatus": {"status": "OK"}, "streamingData": {"adaptiveFormats": [
            {"mimeType": "audio/mp4; codecs=\"mp4a.40.2\"", "bitrate": 130000, "url": "https://a/aac"},
            {"mimeType": "audio/webm; codecs=\"opus\"", "bitrate": 56000, "url": "https://a/low", "contentLength": "100"},
            {"mimeType": "audio/webm; codecs=\"opus\"", "bitrate": 139000, "url": "https://a/high", "contentLength": "3039227"},
            {"mimeType": "audio/webm; codecs=\"opus\"", "bitrate": 999999, "signatureCipher": "s=x"},
            {"mimeType": "video/webm; codecs=\"vp9\"", "bitrate": 9999999, "url": "https://a/video"}
        ]}});
        assert_eq!(pick_stream(&v).unwrap(), Stream { url: "https://a/high".into(), content_length: Some(3_039_227) });
        let blocked = json!({"playabilityStatus": {"status": "LOGIN_REQUIRED", "reason": "Sign in to confirm your age"}});
        assert_eq!(pick_stream(&blocked).unwrap_err(), "YouTube Music: Sign in to confirm your age");
        let aac_only = json!({"playabilityStatus": {"status": "OK"}, "streamingData": {"adaptiveFormats": [
            {"mimeType": "audio/mp4", "bitrate": 1, "url": "https://a/aac"}]}});
        assert!(pick_stream(&aac_only).is_err());
    }

    #[test]
    fn visitor_id_from_sw_data() {
        let body = ")]}'\n[[\"x\",1,[\"a\",[\"CgswdFEtRUZk\"],3]]]";
        assert_eq!(parse_visitor(body).as_deref(), Some("CgswdFEtRUZk"));
        assert_eq!(parse_visitor("[[1,2,[\"nope\"]]]"), None);
    }
}

#[cfg(test)]
mod live {
    /// Talks to YouTube Music: `cargo test --lib music::ytm::live -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn search_resolve_download_demux() {
        let http = super::client();
        let r = super::search(&http, "LANY Stuck").await.unwrap();
        let e = r.first().expect("a result");
        println!("{} — {} ({} ms) {}", e.artist, e.title, e.duration_ms, e.art_url);
        let s = super::resolve(&http, &e.video_id).await.unwrap();
        let t = std::time::Instant::now();
        let data = super::download(&http, &s).await.unwrap();
        let song = crate::music::webm::demux(&data).unwrap();
        println!("{} bytes in {:?}, {} ms of audio", data.len(), t.elapsed(), song.duration_ms());
        assert!((song.duration_ms() as i64 - e.duration_ms as i64).abs() < 3000);
    }
}
