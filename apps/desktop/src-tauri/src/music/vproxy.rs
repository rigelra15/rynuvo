//! The `rynuvo-video://` protocol: music videos reach the player through this app.
//!
//! YouTube throttles open-ended requests ("bytes=N-", what a `<video>` sends) to about twice the
//! video's bitrate, so the picture took seconds to appear. And WebKit's player then reads the file
//! a few KB at a time, one request per frame; sending each of those to YouTube made playback stall
//! whenever one came back slowly. So the file is fetched in aligned `BLOCK`s at full speed and every
//! player request, big or small, is answered from them; the block after the one being read is
//! fetched ahead, so it is in memory by the time the player gets there.
use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock, Mutex},
};

use futures_util::future::{BoxFuture, FutureExt, Shared};
use tauri::{Manager, Runtime, http::Response};

use super::ytm;

/// Bytes fetched from YouTube at a time.
pub const BLOCK: u64 = 2 << 20;

/// The player's `Range` header: start and inclusive end (None = to the end of the file). A suffix
/// range ("bytes=-500") has no start; players don't send one, so it reads from the top.
pub fn parse_range(range: Option<&str>) -> (u64, Option<u64>) {
    let (a, b) = range
        .and_then(|r| r.trim().strip_prefix("bytes="))
        .and_then(|r| r.split(',').next())
        .and_then(|r| r.split_once('-'))
        .map_or((None, None), |(a, b)| (a.trim().parse::<u64>().ok(), b.trim().parse::<u64>().ok()));
    match a {
        Some(a) => (a, b.filter(|b| *b >= a)),
        None => (0, None),
    }
}

/// The file's size, from a "bytes a-e/total" Content-Range.
pub fn total_of(content_range: &str) -> Option<u64> {
    content_range.rsplit_once('/')?.1.trim().parse().ok()
}

/// What to send from the block starting at `block` (holding `len` bytes of a `total`-byte file) for
/// a request from `a` to `b`: the inclusive end, cut at the block's end. None when `a` isn't in it.
pub fn serve_end(block: u64, len: u64, a: u64, b: Option<u64>) -> Option<u64> {
    (len > 0 && a >= block && a < block + len).then(|| b.map_or(block + len - 1, |b| b.min(block + len - 1)))
}

/// Blocks kept (being read or read ahead), across videos: 16 MB at most.
const KEEP: usize = 8;

#[derive(Debug)]
struct Chunk {
    status: u16,
    content_range: Option<String>,
    body: Vec<u8>,
}

type Pending = Shared<BoxFuture<'static, Result<Arc<Chunk>, String>>>;

/// (video id, block start) → the block.
type Blocks = VecDeque<((String, u64), Pending)>;

static CHUNKS: LazyLock<Mutex<Blocks>> = LazyLock::new(Mutex::default);

async fn fetch(http: reqwest::Client, id: String, a: u64, b: u64) -> Result<Arc<Chunk>, String> {
    let url = ytm::resolve_video(&http, &id).await?;
    let r = http.get(&url).header("Range", format!("bytes={a}-{b}")).send().await.map_err(|e| e.to_string())?;
    let status = r.status().as_u16();
    if status != 200 && status != 206 && status != 416 {
        return Err(format!("YouTube answered {status}"));
    }
    let content_range = r.headers().get("content-range").and_then(|v| v.to_str().ok()).map(String::from);
    let body = r.bytes().await.map_err(|e| e.to_string())?.to_vec();
    Ok(Arc::new(Chunk { status, content_range, body }))
}

/// The block starting at `a`: the one already fetched or on its way, else a new fetch.
fn chunk(http: &reqwest::Client, id: &str, a: u64) -> Pending {
    let mut c = CHUNKS.lock().unwrap();
    let key = (id.to_string(), a);
    if let Some((_, p)) = c.iter().find(|(k, _)| *k == key) {
        return p.clone();
    }
    let p = fetch(http.clone(), id.to_string(), a, a + BLOCK - 1).boxed().shared();
    c.push_back((key, p.clone()));
    while c.len() > KEEP {
        c.pop_front();
    }
    p
}

fn forget(id: &str, a: u64) {
    CHUNKS.lock().unwrap().retain(|((v, s), _)| !(v == id && *s == a));
}

fn is_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The block holding byte `a`, kept in memory until evicted; a failed fetch isn't kept.
async fn block_at(http: &reqwest::Client, id: &str, a: u64) -> Result<(u64, Arc<Chunk>), String> {
    let start = a - a % BLOCK;
    let c = chunk(http, id, start).await.inspect_err(|_| forget(id, start))?;
    Ok((start, c))
}

async fn answer(http: &reqwest::Client, id: &str, range: Option<&str>) -> Result<Response<Vec<u8>>, String> {
    if !is_video_id(id) {
        return Err("bad video id".into());
    }
    let (a, b) = parse_range(range);
    let (start, c) = block_at(http, id, a).await?;
    let total = c.content_range.as_deref().and_then(total_of);
    let res = Response::builder().header("content-type", "video/mp4").header("accept-ranges", "bytes").header("cache-control", "no-store");
    let Some(end) = serve_end(start, c.body.len() as u64, a, b).filter(|_| c.status == 206) else {
        // Past the end of the file (or YouTube didn't answer with a range).
        let res = res.status(416);
        let res = match total {
            Some(t) => res.header("content-range", format!("bytes */{t}")),
            None => res,
        };
        return res.body(Vec::new()).map_err(|e| e.to_string());
    };
    // Read ahead: the next block, so it is in memory by the time the player reaches it.
    let next = start + BLOCK;
    if total.is_some_and(|t| next < t) {
        let ahead = chunk(http, id, next);
        let id = id.to_string();
        tauri::async_runtime::spawn(async move {
            if ahead.await.is_err() {
                forget(&id, next);
            }
        });
    }
    let mut body = c.body[(a - start) as usize..=(end - start) as usize].to_vec();
    let mut end = end;
    // A small range across the block's end gets the rest from the next block: the player asked for
    // exactly those bytes and may not ask again for a shortfall.
    if let Some(b) = b.filter(|b| *b > end && b - a < BLOCK) {
        let (s2, c2) = block_at(http, id, end + 1).await?;
        if let Some(e2) = serve_end(s2, c2.body.len() as u64, end + 1, Some(b)).filter(|_| c2.status == 206) {
            body.extend_from_slice(&c2.body[(end + 1 - s2) as usize..=(e2 - s2) as usize]);
            end = e2;
        }
    }
    let total = total.map_or_else(|| "*".to_string(), |t| t.to_string());
    res.status(206).header("content-range", format!("bytes {a}-{end}/{total}")).body(body).map_err(|e| e.to_string())
}

pub fn protocol<R: Runtime>(ctx: tauri::UriSchemeContext<'_, R>, request: tauri::http::Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let id = request.uri().path().trim_start_matches('/').to_string();
    let range = request.headers().get("range").and_then(|v| v.to_str().ok()).map(String::from);
    tauri::async_runtime::spawn(async move {
        let http = app.state::<crate::music_cmd::MusicHttp>().0.clone();
        let res = answer(&http, &id, range.as_deref()).await.unwrap_or_else(|e| {
            tracing::warn!("music video {id}: {e}");
            Response::builder().status(502).body(Vec::new()).unwrap()
        });
        responder.respond(res);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range(Some("bytes=0-")), (0, None));
        assert_eq!(parse_range(Some("bytes=10-19")), (10, Some(19)));
        assert_eq!(parse_range(Some("bytes=-500")), (0, None));
        assert_eq!(parse_range(Some("bytes=30-10")), (30, None), "a backwards end is ignored");
        assert_eq!(parse_range(None), (0, None));
        assert_eq!(parse_range(Some("junk")), (0, None));
        assert_eq!(total_of("bytes 0-2097151/35913463"), Some(35_913_463));
        assert_eq!(total_of("junk"), None);
    }

    #[test]
    fn small_and_open_requests_are_cut_from_the_block() {
        // Block 2 MiB..4 MiB, full.
        let (s, n) = (BLOCK, BLOCK);
        assert_eq!(serve_end(s, n, s + 100, Some(s + 2_099)), Some(s + 2_099), "a few KB, as WebKit asks per frame");
        assert_eq!(serve_end(s, n, s, None), Some(s + n - 1), "open-ended: up to the block's end");
        assert_eq!(serve_end(s, n, s + 10, Some(s + 3 * BLOCK)), Some(s + n - 1), "a range past the block is cut short");
        assert_eq!(serve_end(s, n, s + n, None), None, "not in this block");
        // The file's last block is short.
        assert_eq!(serve_end(s, 1000, s + 999, None), Some(s + 999));
        assert_eq!(serve_end(s, 0, s, None), None);
    }

    #[test]
    fn only_youtube_ids() {
        assert!(is_video_id("EjvAa0RxDLo"));
        assert!(!is_video_id("../etc/pass"));
        assert!(!is_video_id("short"));
    }
}
