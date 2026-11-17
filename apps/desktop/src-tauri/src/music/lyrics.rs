//! Lyrics for the playing song, from the source the voice channel picked: LRCLIB, SimpMusic (by video
//! id), BetterLyrics and LrcRed (Apple Music style TTML, often word-synced), NetEase. "auto" asks them
//! all at once, for every song, and takes LrcRed when it is word-synced, else SimpMusic when it is,
//! else SimpMusic's line-synced lyrics; only when SimpMusic has nothing does it fall back to any other
//! word-synced source, then the first line-synced one, then plain text.
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
    /// When the line ends, when the source says; lines can overlap (duets, backing vocals).
    pub end: Option<u32>,
    pub text: String,
    /// Per-word timing, when the source has it.
    pub words: Vec<Word>,
    /// Backing vocals sung with this line, shown smaller under it.
    pub bg: Vec<Word>,
    /// Sung by the second voice of a duet (shown on the other side).
    pub duet: bool,
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
        lines.push(Line { ms, end: None, text: plain, words, bg: Vec::new(), duet: false });
    }
    lines.sort_by_key(|l| l.ms);
    lines
}

fn nonempty(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// The best SimpMusic entry for this video: closest in length, then the richest timing. Entries are
/// looked up by video id, so one whose stored length is off (it happens) still counts when its
/// timed lyrics fit inside the song.
pub fn pick_simpmusic(v: &Value, duration_ms: u32) -> Option<Lyrics> {
    let mut list: Vec<(i64, &Value)> = v
        .get("data")?
        .as_array()?
        .iter()
        .map(|e| {
            let secs = e.get("durationSeconds").and_then(Value::as_f64);
            (secs.map_or(0, |s| ((s * 1000.0) as i64 - duration_ms as i64).abs()), e)
        })
        .collect();
    list.sort_by_key(|(skew, _)| *skew);
    list.into_iter().find_map(|(skew, e)| {
        let plain = nonempty(e, "plainLyric").map(|p| unescape(&p));
        let lines = ["richSyncLyrics", "syncedLyrics"].iter().find_map(|k| Some(parse_lrc(&nonempty(e, k)?)).filter(|l| !l.is_empty()));
        let fits = |l: &Vec<Line>| l.last().is_some_and(|x| x.ms as i64 <= duration_ms as i64 + MAX_SKEW_MS);
        match lines {
            Some(lines) if skew <= MAX_SKEW_MS || fits(&lines) => Some(Lyrics { lines, plain, source: "SimpMusic" }),
            None if skew <= MAX_SKEW_MS => plain.map(|p| Lyrics { lines: Vec::new(), plain: Some(p), source: "SimpMusic" }),
            _ => None,
        }
    })
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

/// TTML clock ("h:mm:ss.fff", "m:ss.fff", "ss.fff" or "12.3s") → ms.
fn ttml_time(s: &str) -> Option<u32> {
    let mut parts: Vec<&str> = s.trim().trim_end_matches('s').split(':').collect();
    let mut total: f64 = parts.pop()?.parse().ok()?;
    let mut unit = 60.0;
    while let Some(p) = parts.pop() {
        total += p.parse::<f64>().ok()? * unit;
        unit *= 60.0;
    }
    (total >= 0.0).then(|| (total * 1000.0).round() as u32)
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let i = tag.find(&key)? + key.len();
    Some(&tag[i..i + tag[i..].find('"')?])
}

/// The line without its backing vocals (`x-bg` spans), and the backing vocals' inner markup.
fn split_backing(s: &str) -> (String, String) {
    let mut out = String::new();
    let mut bg = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("<span").filter(|_| rest.contains("x-bg")) {
        let end = rest[i..].find('>').map_or(rest.len(), |e| i + e + 1);
        if !rest[i..end].contains("x-bg") {
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        out.push_str(&rest[..i]);
        let (mut depth, mut j) = (1, end);
        while depth > 0 && j < rest.len() {
            let open = rest[j..].find("<span").map(|k| j + k);
            let close = rest[j..].find("</span>").map(|k| j + k);
            match (open, close) {
                (Some(o), Some(c)) if o < c => { depth += 1; j = o + 5; }
                (_, Some(c)) => { depth -= 1; j = c + 7; }
                _ => j = rest.len(),
            }
        }
        // Inside the x-bg span, up to its own closing tag.
        bg.push_str(rest.get(end..j.saturating_sub(7)).unwrap_or(""));
        bg.push(' ');
        rest = &rest[j..];
    }
    out.push_str(rest);
    (out, bg)
}

/// `<span begin>` words; syllables written without a space between them join into one word.
fn spans(body: &str) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut b = body;
    let mut glued = false;
    while let Some(si) = b.find("<span") {
        let gap = &b[..si];
        let Some(te) = b[si..].find('>').map(|e| si + e) else { break };
        let Some(ce) = b[te..].find("</span>").map(|e| te + e) else { break };
        let text = unescape(b[te + 1..ce].trim());
        let begin = attr(&b[si..te], "begin").and_then(ttml_time);
        let spaced = !gap.is_empty() && gap.chars().all(char::is_whitespace) || words.is_empty();
        if let (Some(wms), false) = (begin, text.is_empty() || text.contains('<')) {
            match words.last_mut() {
                Some(last) if glued && !spaced => last.text.push_str(&text),
                _ => words.push(Word { ms: wms, text }),
            }
            glued = true;
        }
        b = &b[ce + 7..];
    }
    words
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    unescape(&out.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Apple Music style TTML (BetterLyrics, LrcRed): `<p begin>` lines, `<span begin>` words. Syllables
/// written without a space between them join into one word.
pub fn parse_ttml(t: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut rest = t;
    while let Some(i) = rest.find("<p ") {
        let Some(open_end) = rest[i..].find('>').map(|e| i + e) else { break };
        let Some(close) = rest[open_end..].find("</p>").map(|e| open_end + e) else { break };
        let tag = &rest[i..open_end];
        let (body, backing) = split_backing(&rest[open_end + 1..close]);
        rest = &rest[close + 4..];
        let Some(ms) = attr(tag, "begin").and_then(ttml_time) else { continue };
        let end = attr(tag, "end").and_then(ttml_time);
        // Apple's agents: v1 the main voice, v2 the duet partner, v1000 everyone.
        let duet = attr(tag, "ttm:agent").is_some_and(|a| a != "v1" && a != "v1000");
        let words = spans(&body);
        let bg = spans(&backing);
        let text = if words.is_empty() { strip_tags(&body) } else { words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ") };
        if !text.is_empty() {
            lines.push(Line { ms, end, text, words, bg, duet });
        }
    }
    lines.sort_by_key(|l| l.ms);
    lines
}

fn timed(lines: Vec<Line>, source: &'static str) -> Option<Lyrics> {
    (!lines.is_empty()).then(|| Lyrics { plain: Some(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")), lines, source })
}

/// The LrcRed hit for this recording (within 10 s of its length): its ISRC.
pub fn pick_lrcred(v: &Value, duration_ms: u32) -> Option<String> {
    v.get("hits")?.as_array()?.iter().find_map(|h| {
        let secs = h.get("duration")?.as_f64()?;
        (((secs * 1000.0) as i64 - duration_ms as i64).abs() <= MAX_SKEW_MS).then(|| nonempty(h, "isrc")).flatten()
    })
}

pub fn pick_netease_song(v: &Value, duration_ms: u32) -> Option<i64> {
    v.get("result")?.get("songs")?.as_array()?.iter().find_map(|s| {
        let d = s.get("duration")?.as_i64()?;
        ((d - duration_ms as i64).abs() <= MAX_SKEW_MS).then(|| s.get("id")?.as_i64()).flatten()
    })
}

pub fn pick_netease(v: &Value) -> Option<Lyrics> {
    timed(parse_lrc(&nonempty(v.get("lrc")?, "lyric")?), "NetEase")
}

/// The title as search engines match it best: no apostrophes ("I’ve" finds nothing on LrcRed,
/// "Ive" does) and no "(feat. …)" / "[Remastered]" extras.
pub fn search_title(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            '\'' | '’' | '‘' | '`' => {}
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Sources to ask, in order, for a channel's choice.
pub fn order(source: &str) -> Vec<&'static str> {
    const AUTO: [&str; 5] = ["lrcred", "simpmusic", "betterlyrics", "lrclib", "netease"];
    AUTO.iter().find(|s| **s == source).map_or_else(|| AUTO.to_vec(), |s| vec![*s])
}

fn ua(r: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    r.header("User-Agent", concat!("Rynuvo/", env!("CARGO_PKG_VERSION")))
}

fn with_query(base: &str, q: &[(&str, &str)]) -> Option<String> {
    url::Url::parse_with_params(base, q).ok().map(String::from)
}

async fn from_source(http: &reqwest::Client, src: &str, video_id: &str, title: &str, artist: &str, duration_ms: u32) -> Option<Lyrics> {
    let first_artist = artist.split(',').next().unwrap_or(artist).trim();
    let secs = (duration_ms / 1000).to_string();
    match src {
        "simpmusic" => {
            let r = http
                .get(format!("https://api-lyrics.simpmusic.org/v1/{video_id}"))
                .header("User-Agent", "SimpMusicLyrics/1.0")
                .header("Accept", "application/json");
            get_json(r).await.and_then(|v| pick_simpmusic(&v, duration_ms))
        }
        "betterlyrics" => {
            let u = with_query("https://lyrics-api.boidu.dev/getLyrics", &[("s", title), ("a", first_artist), ("d", &secs)])?;
            let v = get_json(ua(http.get(u))).await?;
            timed(parse_ttml(v.get("ttml")?.as_str()?), "BetterLyrics")
        }
        "lrcred" => {
            let q = format!("{} {first_artist}", search_title(title));
            let u = with_query("https://lrc.red/search.json", &[("q", &q)])?;
            let isrc = pick_lrcred(&get_json(ua(http.get(u))).await?, duration_ms)?;
            let t = ua(http.get(format!("https://lrc.red/s/{isrc}.ttml"))).send().await.ok()?.error_for_status().ok()?.text().await.ok()?;
            timed(parse_ttml(&t), "LrcRed")
        }
        "lrclib" => {
            let u = with_query("https://lrclib.net/api/get", &[("track_name", title), ("artist_name", first_artist), ("duration", &secs)])?;
            get_json(ua(http.get(u))).await.and_then(|v| pick_lrclib(&v))
        }
        "netease" => {
            let q = format!("{} {first_artist}", search_title(title));
            let u = with_query("https://music.163.com/api/search/get", &[("s", &q), ("type", "1"), ("limit", "5")])?;
            let id = pick_netease_song(&get_json(ua(http.get(u))).await?, duration_ms)?;
            let u = with_query("https://music.163.com/api/song/lyric", &[("id", &id.to_string()), ("lv", "1")])?;
            get_json(ua(http.get(u))).await.and_then(|v| pick_netease(&v))
        }
        _ => None,
    }
}

fn word_synced(l: &Lyrics) -> bool {
    l.lines.iter().any(|x| !x.words.is_empty())
}

/// The best of each source's answer (given in `order` order): LrcRed word-synced, else SimpMusic
/// word-synced, else SimpMusic line-synced; only if SimpMusic had nothing timed, any other
/// word-synced, then the first line-synced, then plain.
pub fn choose(found: Vec<Option<Lyrics>>) -> Option<Lyrics> {
    let found: Vec<Lyrics> = found.into_iter().flatten().collect();
    let from = |src: &str, ok: fn(&Lyrics) -> bool| found.iter().position(|l| l.source == src && ok(l));
    let timed = |l: &Lyrics| !l.lines.is_empty();
    let pick = from("LrcRed", word_synced)
        .or_else(|| from("SimpMusic", word_synced))
        .or_else(|| from("SimpMusic", timed))
        .or_else(|| found.iter().position(word_synced))
        .or_else(|| found.iter().position(timed))
        .or((!found.is_empty()).then_some(0))?;
    found.into_iter().nth(pick)
}

/// Lyrics from the channel's `source`; with "auto", every source is asked at once and `choose`
/// picks the best.
pub async fn fetch(http: &reqwest::Client, source: &str, video_id: &str, title: &str, artist: &str, duration_ms: u32) -> Option<Lyrics> {
    let asks = order(source).into_iter().map(|src| from_source(http, src, video_id, title, artist, duration_ms));
    choose(futures_util::future::join_all(asks).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ttml_word_and_line_timing() {
        let t = r#"<tt><body dur="3:03.8"><div begin="0:10.963"><p begin="0:10.963" end="0:15.2"><span begin="0:10.963" end="0:11.323">Take</span> <span begin="0:11.323" end="0:11.5">a</span> <span ttm:role="x-bg"><span begin="0:12.0" end="0:12.4">(oh)</span></span></p><p begin="15.5" end="20">Line two &amp; more</p><p begin="1:02:03.5">Late</p></div></body></tt>"#;
        let lines = parse_ttml(t);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].ms, 10_963);
        assert_eq!(lines[0].text, "Take a");
        assert_eq!(lines[0].words, vec![Word { ms: 10_963, text: "Take".into() }, Word { ms: 11_323, text: "a".into() }], "backing vocals kept apart");
        assert_eq!(lines[0].bg, vec![Word { ms: 12_000, text: "(oh)".into() }], "backing vocals under the line");
        assert_eq!(lines[0].end, Some(15_200));
        assert_eq!(lines[1], Line { ms: 15_500, end: Some(20_000), text: "Line two & more".into(), words: vec![], bg: vec![], duet: false });
        assert_eq!(lines[2].ms, 3_723_500);
    }

    #[test]
    fn ttml_duets_sing_on_the_other_side() {
        let t = r#"<tt><body><div><p begin="1" end="4" ttm:agent="v1"><span begin="1">Me</span></p><p begin="2" end="5" ttm:agent="v2"><span begin="2">You</span></p><p begin="6" ttm:agent="v1000"><span begin="6">Both</span></p></div></body></tt>"#;
        let l = parse_ttml(t);
        assert_eq!(l.iter().map(|l| l.duet).collect::<Vec<_>>(), [false, true, false]);
        assert_eq!((l[0].end, l[1].ms), (Some(4_000), 2_000), "the lines overlap: both are sung at 3 s");
    }

    #[test]
    fn lrcred_and_netease_match_the_song_length() {
        let hits = serde_json::json!({"hits":[{"isrc":"FAR","duration":120.0},{"isrc":"USA2P2536902","duration":183.803}]});
        assert_eq!(pick_lrcred(&hits, 184_000).as_deref(), Some("USA2P2536902"));
        assert_eq!(pick_lrcred(&hits, 300_000), None);
        let songs = serde_json::json!({"result":{"songs":[{"id":1,"duration":90_000},{"id":279275012,"duration":183803}]}});
        assert_eq!(pick_netease_song(&songs, 184_000), Some(279275012));
        let lyric = serde_json::json!({"lrc":{"lyric":"[00:10.80]Take a deep breath\n[00:16.10]Sun in my eyes"}});
        let l = pick_netease(&lyric).unwrap();
        assert_eq!(l.source, "NetEase");
        assert_eq!(l.lines.len(), 2);
        assert_eq!(pick_netease(&serde_json::json!({"lrc":{"lyric":""}})), None);
    }

    #[test]
    fn search_titles_drop_apostrophes_and_extras() {
        assert_eq!(search_title("Prettiest Thing I’ve Ever Seen"), "Prettiest Thing Ive Ever Seen");
        assert_eq!(search_title("Don't Go (feat. Someone) [Remastered]"), "Dont Go");
        assert_eq!(search_title("Stuck - Live"), "Stuck - Live");
    }

    #[test]
    fn auto_puts_lrcred_then_simpmusic_first() {
        assert_eq!(order("auto"), vec!["lrcred", "simpmusic", "betterlyrics", "lrclib", "netease"]);
        assert_eq!(order("lrclib"), vec!["lrclib"]);
        assert_eq!(order("bogus"), order("auto"));
    }

    #[test]
    fn auto_prefers_word_sync_then_line_sync_then_plain() {
        let word = |src| Some(Lyrics { lines: parse_lrc("[00:01.00] <00:01.00> hi <00:01.50> there"), plain: None, source: src });
        let line = |src| Some(Lyrics { lines: parse_lrc("[00:01.00] hi there"), plain: None, source: src });
        let plain = |src| Some(Lyrics { lines: vec![], plain: Some("hi".into()), source: src });
        let src = |l: Option<Lyrics>| l.map(|l| l.source);
        // Order: lrcred, simpmusic, betterlyrics, lrclib, netease.
        assert_eq!(src(choose(vec![word("LrcRed"), word("SimpMusic"), word("BetterLyrics"), None, None])), Some("LrcRed"));
        assert_eq!(src(choose(vec![line("LrcRed"), word("SimpMusic"), word("BetterLyrics"), None, None])), Some("SimpMusic"), "LrcRed only line-synced");
        assert_eq!(src(choose(vec![None, word("SimpMusic"), None, word("LRCLIB"), None])), Some("SimpMusic"), "no LrcRed");
        assert_eq!(src(choose(vec![line("LrcRed"), line("SimpMusic"), word("BetterLyrics"), word("LRCLIB"), None])), Some("SimpMusic"), "neither word-synced: SimpMusic's lines");
        assert_eq!(src(choose(vec![line("LrcRed"), None, word("BetterLyrics"), None, None])), Some("BetterLyrics"), "no SimpMusic: word sync from anywhere");
        assert_eq!(src(choose(vec![line("LrcRed"), plain("SimpMusic"), None, line("LRCLIB"), None])), Some("LrcRed"), "else the first line-synced");
        assert_eq!(src(choose(vec![plain("LrcRed"), None, None, None, line("NetEase")])), Some("NetEase"), "timed beats plain");
        assert_eq!(src(choose(vec![None, plain("SimpMusic"), None, None, None])), Some("SimpMusic"));
        assert_eq!(choose(vec![None, None]), None);
    }

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
        assert!(pick_simpmusic(&json!({"data": [{"durationSeconds": 240, "syncedLyrics": "[03:30.00] too long"}]}), 120_000).is_none(), "lyrics that outlast the song");
        // Same video, wrong length on file (SimpMusic said 105 s for a 249 s song): the lyrics fit, so use them.
        let off = json!({"data": [{"durationSeconds": 105, "syncedLyrics": "[00:14.84] I couldn't\n[04:05.02] ", "richSyncLyrics": "[00:14.84] <00:15.06>I <00:15.34>couldn't"}]});
        assert_eq!(pick_simpmusic(&off, 249_000).unwrap().lines[0].words.len(), 2);
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
        let http = crate::music::ytm::client();
        for src in ["auto", "simpmusic", "betterlyrics", "lrcred", "lrclib", "netease"] {
            match super::fetch(&http, src, "EjvAa0RxDLo", "Stuck", "LANY", 184_000).await {
                Some(l) => println!("{src}: {} lines ({} word-timed) from {}", l.lines.len(), l.lines.iter().filter(|x| !x.words.is_empty()).count(), l.source),
                None => println!("{src}: none"),
            }
        }
    }

}
