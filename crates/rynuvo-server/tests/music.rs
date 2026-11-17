use rynuvo_proto::{MusicState, WsServerMsg};
use rynuvo_server::testutil::{TestApp, WsClient};
use serde_json::json;

async fn until<T>(ws: &mut WsClient, ms: u64, mut f: impl FnMut(WsServerMsg) -> Option<T>) -> T {
    let end = tokio::time::Instant::now() + std::time::Duration::from_millis(ms);
    loop {
        let left = end.saturating_duration_since(tokio::time::Instant::now()).as_millis() as u64;
        let m = ws.try_next(left.max(1)).await.expect("expected a matching message");
        if let Some(t) = f(m) {
            return t;
        }
    }
}

async fn ready(app: &TestApp, token: &str) -> (WsClient, Option<i64>) {
    let mut ws = app.ws(token).await;
    let bot = until(&mut ws, 3000, |m| match m {
        WsServerMsg::Ready { music_bot, .. } => Some(music_bot),
        _ => None,
    })
    .await;
    (ws, bot)
}

async fn join_voice(ws: &mut WsClient, ch: i64) {
    ws.send_json(json!({"t":"voiceJoin","d":{"channelId":ch,"pubkey":rynuvo_proto::e2ee::DeviceKey::generate().public().to_b64()}})).await;
    until(ws, 3000, |m| matches!(m, WsServerMsg::VoiceJoined { .. }).then_some(())).await;
}

fn add(text: i64, title: &str, ms: u32) -> serde_json::Value {
    json!({"t":"music","d":{"textChannelId":text,"cmd":{"op":"add","entry":{
        "videoId":format!("v{title}"),"title":title,"artist":"LANY","durationMs":ms}}}})
}

async fn bot_says(ws: &mut WsClient, bot: i64) -> String {
    until(ws, 3000, |m| match m {
        WsServerMsg::MessageCreated(m) if m.author_id == bot => Some(m.content),
        _ => None,
    })
    .await
}

async fn music(ws: &mut WsClient, ms: u64) -> MusicState {
    until(ws, ms, |m| match m {
        WsServerMsg::MusicState(s) => Some(s),
        _ => None,
    })
    .await
}

#[tokio::test]
async fn listen_together_over_the_socket() {
    let app = TestApp::spawn().await;
    let poky = app.admin().await;
    let xeno = app.user("Xeno").await;
    let text = app.create_channel(&poky.token, "text", "general").await.id;
    let voice = app.create_channel(&poky.token, "voice", "Playing Room").await.id;
    let (mut p, bot) = ready(&app, &poky.token).await;
    let bot = bot.expect("Ready names the music bot");
    let (mut x, _) = ready(&app, &xeno.token).await;

    // Not in voice yet: the bot says so.
    x.send_json(add(text, "Nope", 5000)).await;
    assert_eq!(bot_says(&mut x, bot).await, "Join a voice channel first");

    join_voice(&mut p, voice).await;
    join_voice(&mut x, voice).await;
    p.send_json(add(text, "Short", 1000)).await;
    let s = music(&mut x, 3000).await;
    assert_eq!(s.channel_id, voice);
    assert_eq!(s.now.as_ref().unwrap().title, "Short");
    assert_eq!(s.started_at - s.server_time, rynuvo_server::music::START_LEAD_MS, "the first song waits for every app to load it");
    p.send_json(add(text, "Next", 60_000)).await;
    assert_eq!(music(&mut x, 3000).await.queue.len(), 1);

    // Xeno's skip of the admin's song is a vote; with two in the call one vote is half the room.
    x.send_json(json!({"t":"music","d":{"textChannelId":text,"cmd":{"op":"skip"}}})).await;
    let mut said = String::new();
    while !said.starts_with("⏭") {
        said = bot_says(&mut x, bot).await;
    }
    assert!(said.starts_with("⏭ Vote passed (1/1): skipped Short\n▶ Now playing: LANY — Next"), "{said}");

    // A late joiner gets the music in Ready.
    let mut late = app.ws(&xeno.token).await;
    let m = until(&mut late, 3000, |m| match m {
        WsServerMsg::Ready { music, .. } => Some(music),
        _ => None,
    })
    .await;
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].now.as_ref().unwrap().title, "Next");
    assert!(m[0].queue.is_empty());

    // Everyone leaves: the music ends.
    p.send_json(json!({"t":"voiceLeave"})).await;
    x.send_json(json!({"t":"voiceLeave"})).await;
    let s = until(&mut late, 3000, |m| match m {
        WsServerMsg::MusicState(s) if s.now.is_none() => Some(s),
        _ => None,
    })
    .await;
    assert!(s.queue.is_empty());
}
