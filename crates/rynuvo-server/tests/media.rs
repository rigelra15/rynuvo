use image::{ImageFormat, RgbaImage};
use rynuvo_proto::{ApiError, ImageRef};
use rynuvo_server::{disk::check_now, testutil::TestApp};
use std::io::Cursor;

fn encode(w: u32, h: u32, fmt: ImageFormat) -> Vec<u8> {
    let img = RgbaImage::from_pixel(w, h, image::Rgba([139, 124, 246, 255]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, fmt).unwrap();
    out.into_inner()
}

async fn upload(app: &TestApp, token: &str, mime: &str, body: Vec<u8>) -> reqwest::Response {
    app.client.post(app.url("/api/v1/media")).bearer_auth(token).header("content-type", mime).body(body).send().await.unwrap()
}

#[tokio::test]
async fn upload_webp_returns_dims() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let r = upload(&app, &u.token, "image/webp", encode(2, 3, ImageFormat::WebP)).await;
    assert_eq!(r.status(), 200);
    let i: ImageRef = r.json().await.unwrap();
    assert_eq!((i.width, i.height), (2, 3));
    assert_eq!(i.mime, "image/webp");
    assert_eq!(i.id.len(), 64);
}

#[tokio::test]
async fn rejects_fake_png() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
    b.extend_from_slice(b"this is not really a png");
    assert_eq!(upload(&app, &u.token, "image/png", b).await.status(), 400);
    // mime mismatch
    assert_eq!(upload(&app, &u.token, "image/png", encode(2, 2, ImageFormat::WebP)).await.status(), 400);
    assert_eq!(upload(&app, &u.token, "text/plain", b"hello".to_vec()).await.status(), 400);
}

#[tokio::test]
async fn rejects_oversize() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let mut b = encode(2, 2, ImageFormat::Png);
    b.resize(5 * 1024 * 1024 + 1, 0);
    let r = upload(&app, &u.token, "image/png", b).await;
    assert_eq!(r.status(), 413);
}

#[tokio::test]
async fn gif_passthrough() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let bytes = encode(4, 5, ImageFormat::Gif);
    let i: ImageRef = upload(&app, &u.token, "image/gif", bytes.clone()).await.json().await.unwrap();
    assert_eq!(i.mime, "image/gif");
    let got = app.client.get(app.url(&format!("/api/v1/media/{}", i.id))).bearer_auth(&u.token).send().await.unwrap();
    assert_eq!(got.headers()["content-type"], "image/gif");
    assert_eq!(got.bytes().await.unwrap().to_vec(), bytes);
}

#[tokio::test]
async fn dedup_same_bytes() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let b = encode(3, 3, ImageFormat::Png);
    let a: ImageRef = upload(&app, &u.token, "image/png", b.clone()).await.json().await.unwrap();
    let c: ImageRef = upload(&app, &u.token, "image/png", b).await.json().await.unwrap();
    assert_eq!(a, c);
}

#[tokio::test]
async fn get_requires_auth() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let i: ImageRef = upload(&app, &u.token, "image/png", encode(2, 2, ImageFormat::Png)).await.json().await.unwrap();
    let r = app.client.get(app.url(&format!("/api/v1/media/{}", i.id))).send().await.unwrap();
    assert_eq!(r.status(), 401);
    let r = app.client.get(app.url("/api/v1/media/..%2F..%2Fetc%2Fpasswd")).bearer_auth(&u.token).send().await.unwrap();
    assert_eq!(r.status(), 404);
}

#[tokio::test]
async fn disk_block_returns_507() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    check_now(&app.state.disk, 93);
    let r = upload(&app, &u.token, "image/png", encode(2, 2, ImageFormat::Png)).await;
    assert_eq!(r.status(), 507);
    assert_eq!(r.json::<ApiError>().await.unwrap().code, "disk_full");
}

#[tokio::test]
async fn profile_picture_must_be_an_uploaded_image() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let patch = |body: serde_json::Value| app.client.patch(app.url("/api/v1/me")).bearer_auth(&u.token).json(&body).send();
    let missing = "a".repeat(64);
    assert_eq!(patch(serde_json::json!({ "avatarImage": missing })).await.unwrap().status(), 400);

    let i: rynuvo_proto::ImageRef = upload(&app, &u.token, "image/png", encode(64, 64, ImageFormat::Png)).await.json().await.unwrap();
    let me: rynuvo_proto::User = patch(serde_json::json!({ "avatarImage": i.id })).await.unwrap().json().await.unwrap();
    assert_eq!(me.avatar_image.as_deref(), Some(i.id.as_str()));
    let me: rynuvo_proto::User = patch(serde_json::json!({ "avatarColor": "teal" })).await.unwrap().json().await.unwrap();
    assert_eq!(me.avatar_image.as_deref(), Some(i.id.as_str()), "other edits keep the picture");
    let me: rynuvo_proto::User = patch(serde_json::json!({ "avatarImage": null })).await.unwrap().json().await.unwrap();
    assert_eq!(me.avatar_image, None);
}
