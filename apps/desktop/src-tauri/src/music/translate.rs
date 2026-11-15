//! Romanization and English translation of lyric lines, from Google Translate's public endpoint.
use serde::Serialize;
use serde_json::Value;

/// What a lyric line says in English, and how it sounds in Latin letters.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    /// The English translation, when the line isn't English already.
    pub translation: Option<String>,
    /// The line in Latin letters, when it's written in another script.
    pub roman: Option<String>,
}

const BASE: &str = "https://clients5.google.com/translate_a";
/// Lines per translation request (keeps the body small).
const CHUNK: usize = 40;
/// Romanization requests in flight at once.
const PARALLEL: usize = 4;

/// Written in a script other than Latin (Hangul, kana, Han, Cyrillic, Thai, Arabic…).
pub fn needs_roman(s: &str) -> bool {
    let latin = |c: char| c.is_ascii() || ('\u{00C0}'..='\u{024F}').contains(&c) || ('\u{1E00}'..='\u{1EFF}').contains(&c);
    s.chars().any(|c| c.is_alphabetic() && !latin(c))
}

const INITIAL: [&str; 19] = ["g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p", "h"];
const MEDIAL: [&str; 21] = ["a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we", "wi", "yu", "eu", "ui", "i"];
/// Final consonants at the end of a syllable (index 0 = none).
const FINAL: [&str; 28] = ["", "k", "k", "k", "n", "n", "n", "t", "l", "k", "m", "l", "l", "l", "p", "l", "m", "p", "p", "t", "t", "ng", "t", "t", "k", "t", "p", "t"];
/// A single final consonant as it sounds when it carries over to a following silent ㅇ.
fn carried(f: usize) -> Option<&'static str> {
    Some(match f {
        1 => "g",
        2 => "kk",
        4 => "n",
        7 => "d",
        8 => "r",
        16 => "m",
        17 => "b",
        19 => "s",
        20 => "ss",
        22 => "j",
        23 => "ch",
        24 => "k",
        25 => "t",
        26 => "p",
        _ => return None,
    })
}

fn syllable(c: char) -> Option<(usize, usize, usize)> {
    let code = (c as u32).checked_sub(0xAC00).filter(|n| *n < 11_172)? as usize;
    Some((code / 588, (code % 588) / 28, code % 28))
}

/// Hangul in Revised Romanization (Korean's official one, what lyric apps show), with the common
/// sound rules: a final consonant moves onto a following silent ㅇ, ㄹ+ㄹ is "ll". Other text stays.
pub fn hangul_roman(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut carry: Option<&str> = None;
    for (i, &c) in chars.iter().enumerate() {
        let Some((ini, med, fin)) = syllable(c) else {
            out.push(c);
            carry = None;
            continue;
        };
        let next = chars.get(i + 1).copied().and_then(syllable);
        match carry.take() {
            Some(onset) if ini == 11 => out.push_str(onset),
            _ if ini == 5 && out.ends_with('l') => out.push('l'),
            _ => out.push_str(INITIAL[ini]),
        }
        out.push_str(MEDIAL[med]);
        match next {
            Some((11, _, _)) if fin == 27 => {}
            Some((11, _, _)) if carried(fin).is_some() => carry = carried(fin),
            _ => out.push_str(FINAL[fin]),
        }
    }
    out
}

/// `[[text, lang], …]` (or one bare `[text, lang]`) → each line's English, unless it was English.
pub fn translations(v: &Value, lines: &[&str]) -> Vec<Option<String>> {
    let wrapped;
    let list = match v.as_array() {
        Some(a) if a.first().is_some_and(Value::is_string) => {
            wrapped = vec![v.clone()];
            &wrapped
        }
        Some(a) => a,
        None => return vec![None; lines.len()],
    };
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let e = list.get(i)?;
            let text = e.get(0)?.as_str()?.trim();
            let lang = e.get(1).and_then(Value::as_str).unwrap_or("");
            let same = text.to_lowercase() == line.trim().to_lowercase();
            (!text.is_empty() && lang != "en" && !same).then(|| text.to_string())
        })
        .collect()
}

/// The romanization in a `translate_a/single?dt=rm` answer: the sentence entry whose 4th field is set.
pub fn roman(v: &Value) -> Option<String> {
    v.get(0)?.as_array()?.iter().find_map(|e| e.get(3)?.as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from))
}

async fn translate_chunk(http: &reqwest::Client, lines: &[&str]) -> Vec<Option<String>> {
    let body = {
        let mut b = url::form_urlencoded::Serializer::new(String::new());
        for l in lines {
            b.append_pair("q", l);
        }
        b.finish()
    };
    let r = http
        .post(format!("{BASE}/t?client=dict-chrome-ex&sl=auto&tl=en"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;
    match r {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(v) => translations(&v, lines),
            Err(_) => vec![None; lines.len()],
        },
        _ => vec![None; lines.len()],
    }
}

async fn romanize(http: &reqwest::Client, line: &str) -> Option<String> {
    let u = url::Url::parse_with_params(&format!("{BASE}/single"), &[("client", "dict-chrome-ex"), ("sl", "auto"), ("tl", "en"), ("dt", "t"), ("dt", "rm"), ("q", line)]).ok()?;
    let r = http.get(u.as_str()).timeout(std::time::Duration::from_secs(10)).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    roman(&r.json::<Value>().await.ok()?)
}

/// Translation and romanization for each line (same order). Repeated lines are asked once; a
/// failed request just leaves its lines without.
pub async fn annotate(http: &reqwest::Client, lines: &[String]) -> Vec<Annotation> {
    use std::collections::HashMap;
    let mut unique: Vec<&str> = Vec::new();
    for l in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        if !unique.contains(&l) {
            unique.push(l);
        }
    }
    let mut out: HashMap<&str, Annotation> = unique.iter().map(|l| (*l, Annotation::default())).collect();
    for chunk in unique.chunks(CHUNK) {
        for (l, t) in chunk.iter().zip(translate_chunk(http, chunk).await) {
            out.get_mut(l).unwrap().translation = t;
        }
    }
    // Korean is romanized here (properly); other scripts by Google.
    let korean = |l: &str| l.chars().any(|c| syllable(c).is_some()) && !needs_roman(&l.chars().filter(|c| syllable(*c).is_none()).collect::<String>());
    for l in unique.iter().filter(|l| korean(l)) {
        out.get_mut(l).unwrap().roman = Some(hangul_roman(l));
    }
    let asks: Vec<String> = unique.iter().filter(|l| needs_roman(l) && !korean(l)).map(|l| l.to_string()).collect();
    let mut romans: Vec<(String, Option<String>)> = Vec::new();
    for batch in asks.chunks(PARALLEL) {
        let got = futures_util::future::join_all(batch.iter().map(|l| romanize(http, l))).await;
        romans.extend(batch.iter().cloned().zip(got));
    }
    for (l, r) in romans {
        if let Some(a) = out.get_mut(l.as_str()) {
            a.roman = r;
        }
    }
    lines.iter().map(|l| out.get(l.trim()).cloned().unwrap_or_default()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_other_scripts_get_romanized() {
        assert!(needs_roman("사랑해요"));
        assert!(needs_roman("夜に駆ける"));
        assert!(needs_roman("Я тебя люблю"));
        assert!(!needs_roman("I'm stuck on you, oh-oh!"));
        assert!(!needs_roman("Café à l'été 2024"));
        assert!(!needs_roman(""));
    }

    #[test]
    fn batch_translations_line_up_and_skip_english() {
        let v = json!([["i love you", "ko"], ["I love you", "en"], ["I love you", "id"]]);
        let lines = ["사랑해요", "I love you", "Aku cinta kamu"];
        assert_eq!(translations(&v, &lines), vec![Some("i love you".into()), None, Some("I love you".into())]);
        // One line comes back unwrapped.
        assert_eq!(translations(&json!(["your name is", "ja"]), &["君の名は"]), vec![Some("your name is".into())]);
        assert_eq!(translations(&json!("nonsense"), &["a"]), vec![None]);
    }

    #[test]
    fn romanization_is_read_from_the_sentence_list() {
        let v = json!([[["Running into the night with you", "夜に駆ける 君と", null, null, 3], [null, null, null, "Yoru ni kakeru-kun to"]], null, "ja"]);
        assert_eq!(roman(&v).as_deref(), Some("Yoru ni kakeru-kun to"));
        assert_eq!(roman(&json!([[["hi", "hi"]], null, "en"])), None);
    }

    #[test]
    fn hangul_follows_revised_romanization() {
        assert_eq!(hangul_roman("사랑해요"), "saranghaeyo");
        assert_eq!(hangul_roman("안녕하세요"), "annyeonghaseyo");
        assert_eq!(hangul_roman("음악"), "eumak", "a final consonant carries over to a silent ㅇ");
        assert_eq!(hangul_roman("한국어"), "hangugeo");
        assert_eq!(hangul_roman("별로"), "byeollo");
        assert_eq!(hangul_roman("I love 너 baby"), "I love neo baby");
    }

    #[tokio::test]
    #[ignore = "live: asks Google Translate"]
    async fn annotates_live() {
        let http = crate::music::ytm::client();
        let lines = ["사랑해요".to_string(), "I love you".into(), "夜に駆ける".into(), "".into(), "사랑해요".into()];
        for (l, a) in lines.iter().zip(annotate(&http, &lines).await) {
            println!("{l:?} → {a:?}");
        }
    }
}
