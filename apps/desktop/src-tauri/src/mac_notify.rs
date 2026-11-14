//! macOS notifications through UNUserNotificationCenter.
//! tauri-plugin-notification uses the deprecated NSUserNotification API, which delivers nothing on macOS 27.
use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest, UNNotificationSound,
    UNUserNotificationCenter,
};

/// UNUserNotificationCenter aborts the process when there is no app bundle (e.g. `cargo run`).
fn in_bundle() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

fn describe(err: *mut NSError) -> String {
    // SAFETY: called only with a non-null NSError handed to a completion block by the framework.
    let e = unsafe { &*err };
    format!("{} {} {}", e.domain(), e.code(), e.localizedDescription())
}

/// Ask once at startup; macOS shows its permission prompt only the first time.
pub fn request_permission() {
    if !in_bundle() {
        return;
    }
    let done = RcBlock::new(|granted: Bool, err: *mut NSError| {
        if err.is_null() {
            tracing::info!(granted = granted.as_bool(), "notification permission");
        } else {
            tracing::warn!("notification permission request failed: {}", describe(err));
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound, &done);
}

pub fn show(title: &str, body: &str) {
    if !in_bundle() {
        return;
    }
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    content.setSound(Some(&UNNotificationSound::defaultSound()));
    let id = NSString::from_str(&format!("rynuvo-{}", crate::core::now_ms()));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
    let done = RcBlock::new(|err: *mut NSError| {
        if err.is_null() {
            tracing::info!("notification queued");
        } else {
            tracing::warn!("notification was not delivered: {}", describe(err));
        }
    });
    UNUserNotificationCenter::currentNotificationCenter().addNotificationRequest_withCompletionHandler(&request, Some(&done));
}
