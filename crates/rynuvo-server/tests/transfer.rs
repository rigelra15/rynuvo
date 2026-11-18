use rynuvo_proto::{TransferCodeResp, WsServerMsg};
use rynuvo_server::testutil::TestApp;
use serde_json::json;

/// The device showing a transfer code hears when it's used, and by what.
#[tokio::test]
async fn using_a_transfer_code_tells_the_device_that_made_it() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let mut ws = app.ws(&u.token).await;
    assert!(matches!(ws.next().await, WsServerMsg::Ready { .. }));

    let t: TransferCodeResp = app.client.post(app.url("/api/v1/me/transfer-code")).bearer_auth(&u.token).send().await.unwrap().json().await.unwrap();
    let r = app.client.post(app.url("/api/v1/claim-transfer")).json(&json!({"code": t.code, "deviceName": "Android"})).send().await.unwrap();
    assert!(r.status().is_success());

    let mut heard = None;
    while let Some(m) = ws.try_next(800).await {
        if let WsServerMsg::TransferClaimed { expires_at, device } = m {
            heard = Some((expires_at, device));
            break;
        }
    }
    assert_eq!(heard, Some((t.expires_at, "Android".to_string())));
}

async fn invited(ws: &mut rynuvo_server::testutil::WsClient) -> Option<(i64, i64)> {
    while let Some(m) = ws.try_next(600).await {
        if let WsServerMsg::VoiceInvited { channel_id, from } = m {
            return Some((channel_id, from));
        }
    }
    None
}

/// "Come hang out": only from someone in a voice channel, not to those already there, not twice in a row.
#[tokio::test]
async fn voice_invites_reach_friends_once() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Budi").await;
    let v = app.create_channel(&a.token, "voice", "ngobrol").await;
    let mut wa = app.ws(&a.token).await;
    let mut wb = app.ws(&b.token).await;
    wa.next().await;
    wb.next().await;

    // Not in voice yet: nothing goes out.
    wa.send_json(json!({"t":"voiceInvite","d":{"to":[b.user.id]}})).await;
    assert_eq!(invited(&mut wb).await, None);

    wa.send_json(json!({"t":"voiceJoin","d":{"channelId":v.id,"pubkey":rynuvo_proto::e2ee::DeviceKey::generate().public().to_b64()}})).await;
    while let Some(m) = wa.try_next(1500).await {
        if matches!(m, WsServerMsg::VoiceJoined { .. }) {
            break;
        }
    }
    wa.send_json(json!({"t":"voiceInvite","d":{"to":[b.user.id, b.user.id, a.user.id]}})).await;
    assert_eq!(invited(&mut wb).await, Some((v.id, a.user.id)));
    assert_eq!(invited(&mut wa).await, None, "not to myself");

    // Again right away: held back.
    wa.send_json(json!({"t":"voiceInvite","d":{"to":[b.user.id]}})).await;
    assert_eq!(invited(&mut wb).await, None);
}

/// A phone with the app closed (or in the background) gets a push; one with it open doesn't. The
/// push carries only an id, and the phone's push key fetches the notification, for its own user only.
#[tokio::test]
async fn pushes_go_to_phones_without_the_app_open() {
    use rynuvo_proto::{PushNote, PushRegisterResp};
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Budi").await;
    let ch = app.create_channel(&a.token, "text", "umum").await;

    let reg: PushRegisterResp = app.client.post(app.url("/api/v1/me/push")).bearer_auth(&b.token).json(&json!({"token": "fcm-budi"})).send().await.unwrap().json().await.unwrap();
    let post = |text: &'static str| {
        let (app, token) = (&app, a.token.clone());
        async move {
            app.client.post(app.url(&format!("/api/v1/channels/{}/messages", ch.id))).bearer_auth(token).json(&json!({"content": text})).send().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    };

    // Budi's app is open: no push.
    let mut wb = app.ws(&b.token).await;
    wb.next().await;
    post("halo").await;
    assert!(app.state.push.sent().is_empty());

    // In the background: pushed, and the note says who and what (with the @mention).
    wb.send_json(json!({"t":"away","d":{"away":true}})).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    post("@Budi main yuk").await;
    let sent = app.state.push.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "fcm-budi");
    let note: PushNote = app.client.get(app.url(&format!("/api/v1/push/notes/{}", sent[0].1))).header("x-push-key", &reg.key).send().await.unwrap().json().await.unwrap();
    assert_eq!((note.from.as_str(), note.text.as_str(), note.channel.as_str(), note.mention), ("Dimas", "@Budi main yuk", "umum", true));

    // Without the key, or with a wrong one: nothing.
    let r = app.client.get(app.url(&format!("/api/v1/push/notes/{}", sent[0].1))).header("x-push-key", "nope").send().await.unwrap();
    assert_eq!(r.status(), 401);

    // The author never gets their own; closed app gets it; signed off gets nothing.
    drop(wb);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    post("lagi").await;
    assert_eq!(app.state.push.sent().len(), 2);
    app.client.delete(app.url("/api/v1/me/push")).bearer_auth(&b.token).send().await.unwrap();
    post("sepi").await;
    assert_eq!(app.state.push.sent().len(), 2);
}

/// My own message never buzzes my own phone, even with the app closed there.
#[tokio::test]
async fn no_push_for_my_own_messages() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let ch = app.create_channel(&a.token, "text", "umum").await;
    app.client.post(app.url("/api/v1/me/push")).bearer_auth(&a.token).json(&json!({"token": "fcm-dimas"})).send().await.unwrap();
    app.client.post(app.url(&format!("/api/v1/channels/{}/messages", ch.id))).bearer_auth(&a.token).json(&json!({"content": "hai"})).send().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(app.state.push.sent().is_empty(), "{:?}", app.state.push.sent());
}

/// A hangout rings a closed-app phone once; whoever comes after updates it quietly; back within two
/// minutes of leaving is a hiccup, not news; once the room has stayed empty, the notification goes.
#[tokio::test]
async fn room_hangouts_ring_once_then_update_quietly() {
    use rynuvo_proto::{PushNote, PushRegisterResp};
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Budi").await;
    let c = app.user("Citra").await;
    let v = app.create_channel(&a.token, "voice", "ngobrol").await;
    let reg: PushRegisterResp = app.client.post(app.url("/api/v1/me/push")).bearer_auth(&b.token).json(&json!({"token": "fcm-budi"})).send().await.unwrap().json().await.unwrap();
    let join = || json!({"t":"voiceJoin","d":{"channelId":v.id,"pubkey":rynuvo_proto::e2ee::DeviceKey::generate().public().to_b64()}});
    let pause = |ms| tokio::time::sleep(std::time::Duration::from_millis(ms));
    let note = |id: String| {
        let (app, key) = (&app, reg.key.clone());
        async move { app.client.get(app.url(&format!("/api/v1/push/notes/{id}"))).header("x-push-key", key).send().await.unwrap().json::<PushNote>().await.unwrap() }
    };

    let mut wa = app.ws(&a.token).await;
    let mut wc = app.ws(&c.token).await;
    wa.next().await;
    wc.next().await;

    // Dimas starts it: Budi's phone rings.
    wa.send_json(join()).await;
    pause(300).await;
    let sent = app.state.push.sent();
    assert_eq!(sent.len(), 1);
    let n = note(sent[0].1.clone()).await;
    assert_eq!((n.kind.as_str(), n.quiet, n.people.len()), ("room", false, 1));

    // Citra comes too: the same notification, now with both, without a sound.
    wc.send_json(join()).await;
    pause(300).await;
    let sent = app.state.push.sent();
    assert_eq!(sent.len(), 2);
    let n = note(sent[1].1.clone()).await;
    assert_eq!((n.quiet, n.from.as_str(), n.people.len()), (true, "Citra", 2));

    // Citra drops and is straight back: nothing.
    wc.send_json(json!({"t":"voiceLeave"})).await;
    pause(150).await;
    wc.send_json(join()).await;
    pause(300).await;
    assert_eq!(app.state.push.sent().len(), 2);

    // Everyone goes; after the grace the notification is cleared (a push without a note).
    wa.send_json(json!({"t":"voiceLeave"})).await;
    wc.send_json(json!({"t":"voiceLeave"})).await;
    pause(700).await;
    let sent = app.state.push.sent();
    assert_eq!(sent.len(), 3);
    assert_eq!(sent[2], ("fcm-budi".to_string(), String::new()));
}

/// A reply typed into a notification posts as the phone's user; a wrong key posts nothing.
#[tokio::test]
async fn replying_from_a_notification() {
    use rynuvo_proto::{Message, PushRegisterResp};
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Budi").await;
    let ch = app.create_channel(&a.token, "text", "umum").await;
    let reg: PushRegisterResp = app.client.post(app.url("/api/v1/me/push")).bearer_auth(&b.token).json(&json!({"token": "fcm-budi"})).send().await.unwrap().json().await.unwrap();

    let r = app.client.post(app.url("/api/v1/push/reply")).header("x-push-key", "nope").json(&json!({"channelId": ch.id, "content": "x"})).send().await.unwrap();
    assert_eq!(r.status(), 401);
    let r = app.client.post(app.url("/api/v1/push/reply")).header("x-push-key", &reg.key).json(&json!({"channelId": ch.id, "content": "otw!"})).send().await.unwrap();
    assert_eq!(r.status(), 204);

    let list: Vec<Message> = app.client.get(app.url(&format!("/api/v1/channels/{}/messages", ch.id))).bearer_auth(&a.token).send().await.unwrap().json().await.unwrap();
    let last = list.last().unwrap();
    assert_eq!((last.author_id, last.content.as_str()), (b.user.id, "otw!"));
}
