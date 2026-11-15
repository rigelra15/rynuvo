//! Lyrics for the playing song: SimpMusic first (by video id, word-synced when it has them), then
//! LRCLIB (by title, artist and length).
use serde::Serialize;
use serde_json::Value;

/// A SimpMusic entry this far off the song's length is a different recording.
const MAX_SKEW_MS: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub ms: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub ms: u32,
    pub text: String,
    /// Per-word timing, when the source has it.
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    /// Timed lines; empty when only plain lyrics exist.
    pub lines: Vec<Line>,
    pub plain: Option<String>,
    pub source: &'static str,
}

/// "mm:ss.xx" → ms.
fn stamp(s: &str) -> Option<u32> {
    let (m, rest) = s.split_once(':')?;
    let (sec, frac) = rest.split_once('.').unwrap_or((rest, "0"));
    let frac_ms = match frac.len() {
        1 => frac.parse::<u32>().ok()? * 100,
        2 => frac.parse::<u32>().ok()? * 10,
        _ => frac.get(..3)?.parse::<u32>().ok()?,
    };
    Some(m.trim().parse::<u32>().ok()? * 60_000 + sec.parse::<u32>().ok()? * 1000 + frac_ms)
}

/// Undo HTML escaping (SimpMusic sends `don&#x27;t`).
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|e| *e <= 10).and_then(|e| {
            let c = match &rest[1..e] {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
                n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            }?;
            Some((c, e + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// LRC text, with optional `<mm:ss.xx>` word stamps (SimpMusic's rich sync).
pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let Some(rest) = raw.strip_prefix('[') else { continue };
        let Some((tag, body)) = rest.split_once(']') else { continue };
        let Some(ms) = stamp(tag) else { continue };
        let mut words = Vec::new();
        let mut plain = String::new();
        if body.contains('<') {
            for seg in body.split('<').skip(1) {
                let Some((t, w)) = seg.split_once('>') else { continue };
                let w = w.trim();
                if let (Some(t), false) = (stamp(t), w.is_empty()) {
                    words.push(Word { ms: t, text: unescape(w) });
                }
            }
            plain = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
        } else {
            plain.push_str(&unescape(body.trim()));
        }
        lines.push(Line { ms, text: plain, words });
    }
    lines.sort_by_key(|l| l.ms);
    lines
}

fn nonempty(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// The best SimpMusic entry: closest in length (within 10 s), then the richest timing.
pub fn pick_simpmusic(v: &Value, duration_ms: u32) -> Option<Lyrics> {
    let list = v.get("data")?.as_array()?;
    let best = list
        .iter()
        .filter_map(|e| {
            let secs = e.get("durationSeconds").and_then(Value::as_f64);
            let skew = secs.map_or(0, |s| ((s * 1000.0) as i64 - duration_ms as i64).abs());
            (skew <= MAX_SKEW_MS).then_some((skew, e))
        })
        .min_by_key(|(skew, _)| *skew)?
        .1;
    let plain = nonempty(best, "plainLyric").map(|p| unescape(&p));
    for key in ["richSyncLyrics", "syncedLyrics"] {
        if let Some(t) = nonempty(best, key) {
            let lines = parse_lrc(&t);
            if !lines.is_empty() {
                return Some(Lyrics { lines, plain, source: "SimpMusic" });
            }
        }
    }
    plain.map(|p| Lyrics { lines: Vec::new(), plain: Some(p), source: "SimpMusic" })
}

pub fn pick_lrclib(v: &Value) -> Option<Lyrics> {
    if v.get("instrumental").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let plain = nonempty(v, "plainLyrics");
    let lines = nonempty(v, "syncedLyrics").map(|t| parse_lrc(&t)).unwrap_or_default();
    (!lines.is_empty() || plain.is_some()).then_some(Lyrics { lines, plain, source: "LRCLIB" })
}

async fn get_json(req: reqwest::RequestBuilder) -> Option<Value> {
    let r = req.timeout(std::time::Duration::from_secs(8)).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    r.json().await.ok()
}

pub async fn fetch(http: &reqwest::Client, video_id: &str, title: &str, artist: &str, duration_ms: u32) -> Option<Lyrics> {
    let simp = http
        .get(format!("https://api-lyrics.simpmusic.org/v1/{video_id}"))
        .header("User-Agent", "SimpMusicLyrics/1.0")
        .header("Accept", "application/json");
    if let Some(l) = get_json(simp).await.and_then(|v| pick_simpmusic(&v, duration_ms)) {
        return Some(l);
    }
    let first_artist = artist.split(',').next().unwrap_or(artist).trim();
    let url = url::Url::parse_with_params(
        "https://lrclib.net/api/get",
        &[("track_name", title), ("artist_name", first_artist), ("duration", &(duration_ms / 1000).to_string())],
    )
    .ok()?;
    let lrclib = http.get(url.as_str()).header("User-Agent", concat!("Rynuvo/", env!("CARGO_PKG_VERSION")));
    get_json(lrclib).await.and_then(|v| pick_lrclib(&v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn html_entities_are_decoded() {
        let l = parse_lrc("[00:01.00] But you don&#x27;t &amp; I &quot;won&#39;t&quot; &lt;3\n[00:02.00] <00:02.00> don&#x27;t <00:02.50>");
        assert_eq!(l[0].text, "But you don't & I \"won't\" <3");
        assert_eq!(l[1].words[0].text, "don't");
    }

    #[test]
    fn lrc_lines_and_rich_words() {
        let l = parse_lrc("[ar:LANY]\n[00:13.04] Drive around town\n[00:11.04] Take a deep breath\n[01:02.5]\nnoise");
        assert_eq!(l.iter().map(|x| (x.ms, x.text.as_str())).collect::<Vec<_>>(), vec![(11_040, "Take a deep breath"), (13_040, "Drive around town"), (62_500, "")]);
        let r = parse_lrc("[offset:+0.0001]\n[00:11.46] <00:11.46> Take <00:11.55>   <00:11.67> a <00:11.72>   <00:11.75> deep <00:11.84>");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].text, "Take a deep");
        assert_eq!(r[0].words, vec![Word { ms: 11_460, text: "Take".into() }, Word { ms: 11_670, text: "a".into() }, Word { ms: 11_750, text: "deep".into() }]);
    }

    #[test]
    fn simpmusic_picks_the_closest_length_and_richest_timing() {
        let v = json!({"data": [
            {"durationSeconds": 240, "syncedLyrics": "[00:01.00] wrong version"},
            {"durationSeconds": 179, "plainLyric": "Take a deep breath", "syncedLyrics": "[00:11.04] Take a deep breath",
             "richSyncLyrics": "[00:11.46] <00:11.46> Take <00:11.55>"},
        ]});
        let l = pick_simpmusic(&v, 184_000).unwrap();
        assert_eq!(l.source, "SimpMusic");
        assert_eq!(l.lines[0].words.len(), 1, "word-synced preferred");
        assert_eq!(l.plain.as_deref(), Some("Take a deep breath"));
        assert!(pick_simpmusic(&v, 120_000).is_none(), "nothing within 10 s");
        let synced = json!({"data": [{"durationSeconds": 184, "syncedLyrics": "[00:11.04] a", "richSyncLyrics": ""}]});
        assert!(pick_simpmusic(&synced, 184_000).unwrap().lines[0].words.is_empty());
        let plain = json!({"data": [{"plainLyric": "just words"}]});
        assert_eq!(pick_simpmusic(&plain, 1).unwrap().lines.len(), 0);
        assert!(pick_simpmusic(&json!({"success": false}), 1).is_none());
    }

    #[test]
    fn lrclib_synced_then_plain() {
        let l = pick_lrclib(&json!({"syncedLyrics": "[00:11.04] Take", "plainLyrics": "Take"})).unwrap();
        assert_eq!((l.lines.len(), l.source), (1, "LRCLIB"));
        assert_eq!(pick_lrclib(&json!({"plainLyrics": "Take", "syncedLyrics": null})).unwrap().lines.len(), 0);
        assert!(pick_lrclib(&json!({"instrumental": true, "plainLyrics": "x"})).is_none());
        assert!(pick_lrclib(&json!({"statusCode": 404})).is_none());
    }
}

#[cfg(test)]
mod live {
    /// `cargo test --lib music::lyrics::live -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn fetches_stuck() {
        let l = super::fetch(&crate::music::ytm::client(), "EjvAa0RxDLo", "Stuck", "LANY", 184_000).await.unwrap();
        println!("{} lines from {}, first: {:?}", l.lines.len(), l.source, l.lines.first());
        let l = super::fetch(&crate::music::ytm::client(), "xxxxxxxxxxx", "Stuck", "LANY", 184_000).await.unwrap();
        println!("fallback: {} lines from {}", l.lines.len(), l.source);
    }
}
