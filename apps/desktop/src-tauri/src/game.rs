//! Is a game in front? The call overlay shows only then (like Discord's), without hooking into the
//! game: the app in front is looked at from outside, once a second.
//!
//! A game is an app that says so (macOS: the App Store's Games category), one Windows knows as a
//! game (the list Game Bar keeps), or one run from a game store's library (Steam, Epic, GOG, …) or
//! through Wine.
use std::time::Duration;

use tauri::{AppHandle, Emitter, Runtime};

/// To the main window: `true` while a game is in front.
pub const EVENT: &str = "rynuvo://game";
/// To the main window: the game being played (its name), or `null`. It stays while the game runs,
/// in front or not.
pub const ACTIVITY_EVENT: &str = "rynuvo://activity";

/// Where game stores install games. Matched against the lowercased path with `/` separators.
const LIBRARIES: &[&str] = &[
    "/steamapps/common/",
    "/epic games/",
    "/gog games/",
    "/gog galaxy/games/",
    "/riot games/",
    "/xboxgames/",
    "/ubisoft game launcher/games/",
    "/ea games/",
    "/origin games/",
    "/heroic/",
    "/crossover/",
    "/whisky/",
];

/// Launchers that live beside the games but aren't one.
const NOT_GAMES: &[&str] = &["/riot client/", "steamwebhelper", "epicgameslauncher", "/steam.exe", "/steam.app/"];

/// The verdict for an app in front, from its executable's path, its declared category (macOS) and
/// the paths Windows knows as games.
pub fn is_game(exe: &str, category: Option<&str>, known: &[String]) -> bool {
    let path = exe.replace('\\', "/").to_lowercase();
    if NOT_GAMES.iter().any(|n| path.contains(n)) {
        return false;
    }
    if category.is_some_and(|c| c.to_lowercase().contains("games")) {
        return true;
    }
    if LIBRARIES.iter().any(|l| path.contains(l)) {
        return true;
    }
    let name = path.rsplit('/').next().unwrap_or("");
    if name.starts_with("wine") {
        return true;
    }
    known.iter().any(|k| k.replace('\\', "/").to_lowercase() == path)
}

/// A game's name for others to see: its folder in a store's library (that's the title, e.g.
/// `steamapps/common/Euro Truck Simulator 2/…`), else the app's own name.
pub fn game_name(exe: &str, app_name: Option<&str>) -> String {
    let path = exe.replace('\\', "/");
    let lower = path.to_lowercase();
    for l in LIBRARIES {
        if let Some(i) = lower.find(l) {
            let rest = &path[i + l.len()..];
            if let Some(dir) = rest.split('/').next().filter(|d| !d.is_empty()) {
                return dir.trim_end_matches(".app").to_string();
            }
        }
    }
    if let Some(n) = app_name.map(str::trim).filter(|n| !n.is_empty()) {
        return n.to_string();
    }
    let file = path.rsplit('/').next().unwrap_or(&path);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_string()
}

/// Watches the app in front and tells the main window when a game comes or goes.
pub fn watch<R: Runtime>(app: AppHandle<R>) {
    std::thread::Builder::new()
        .name("game-watch".into())
        .spawn(move || {
            let mut known = Vec::new();
            let mut known_at: Option<std::time::Instant> = None;
            let mut was = false;
            // The game being played: its process and name.
            let mut playing: Option<(u32, String)> = None;
            loop {
                // Windows' list changes only when a new game is played: read it now and then.
                if known_at.is_none_or(|t| t.elapsed() > Duration::from_secs(120)) {
                    known = os::known_games();
                    known_at = Some(std::time::Instant::now());
                }
                let front = os::front().filter(|f| is_game(&f.exe, f.category.as_deref(), &known));
                let now = front.is_some();
                if now != was {
                    was = now;
                    tracing::info!("game in front: {now}");
                    let _ = app.emit_to("main", EVENT, now);
                }
                let next = match front {
                    Some(f) => Some((f.pid, game_name(&f.exe, f.name.as_deref()))),
                    None => playing.clone().filter(|(pid, _)| os::alive(*pid)),
                };
                if next.as_ref().map(|p| &p.1) != playing.as_ref().map(|p| &p.1) {
                    tracing::info!("playing: {:?}", next.as_ref().map(|p| &p.1));
                    let _ = app.emit_to("main", ACTIVITY_EVENT, next.as_ref().map(|p| p.1.clone()));
                }
                playing = next;
                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .ok();
}

/// The app in front (not Rynuvo).
pub struct Front {
    pid: u32,
    exe: String,
    /// The category it declares (macOS).
    category: Option<String>,
    /// Its display name (macOS).
    name: Option<String>,
}

#[cfg(target_os = "macos")]
mod os {
    use super::Front;
    use objc2::{class, msg_send, rc::autoreleasepool, runtime::AnyObject};
    use objc2_foundation::{NSBundle, NSString};

    unsafe fn path_of(url: *mut AnyObject) -> Option<String> {
        if url.is_null() {
            return None;
        }
        let p: *mut NSString = unsafe { msg_send![&*url, path] };
        (!p.is_null()).then(|| unsafe { &*p }.to_string())
    }

    /// The app in front (not Rynuvo): its executable, the category its Info.plist declares, its name.
    pub fn front() -> Option<Front> {
        autoreleasepool(|_| unsafe {
            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            if ws.is_null() {
                return None;
            }
            let app: *mut AnyObject = msg_send![&*ws, frontmostApplication];
            if app.is_null() {
                return None;
            }
            let pid: i32 = msg_send![&*app, processIdentifier];
            if pid == std::process::id() as i32 {
                return None;
            }
            let exe = path_of(msg_send![&*app, executableURL])?;
            let category = path_of(msg_send![&*app, bundleURL]).and_then(|b| {
                let bundle = NSBundle::bundleWithPath(&NSString::from_str(&b))?;
                let v = bundle.objectForInfoDictionaryKey(&NSString::from_str("LSApplicationCategoryType"))?;
                v.downcast::<NSString>().ok().map(|s| s.to_string())
            });
            let name: *mut NSString = msg_send![&*app, localizedName];
            let name = (!name.is_null()).then(|| (&*name).to_string());
            Some(Front { pid: pid as u32, exe, category, name })
        })
    }

    pub fn alive(pid: u32) -> bool {
        autoreleasepool(|_| unsafe {
            let app: *mut AnyObject = msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid as i32];
            !app.is_null() && !msg_send![&*app, isTerminated]
        })
    }

    pub fn known_games() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(windows)]
mod os {
    use super::Front;
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    };

    /// The app in front (not Rynuvo): its executable. (Windows apps declare no category.)
    pub fn front() -> Option<Front> {
        unsafe {
            let w = GetForegroundWindow();
            if w.is_invalid() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(w, Some(&mut pid));
            if pid == 0 || pid == std::process::id() {
                return None;
            }
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut n = buf.len() as u32;
            let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n);
            let _ = CloseHandle(h);
            r.ok()?;
            Some(Front { pid, exe: String::from_utf16_lossy(&buf[..n as usize]), category: None, name: None })
        }
    }

    pub fn alive(pid: u32) -> bool {
        const STILL_ACTIVE: u32 = 259;
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code).is_ok();
            let _ = CloseHandle(h);
            ok && code == STILL_ACTIVE
        }
    }

    /// The games Windows has recognised (Game Bar's list): their executables.
    pub fn known_games() -> Vec<String> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let Ok(out) = std::process::Command::new("reg")
            .args(["query", r"HKCU\System\GameConfigStore\Children", "/s", "/v", "MatchedExeFullPath"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            return Vec::new();
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_string()))
            .filter(|v| !v.is_empty())
            .collect()
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod os {
    use super::Front;
    pub fn front() -> Option<Front> {
        None
    }
    pub fn alive(_: u32) -> bool {
        false
    }
    pub fn known_games() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{game_name, is_game};

    #[test]
    fn game_names() {
        assert_eq!(game_name(r"C:\Program Files (x86)\Steam\steamapps\common\Brawlhalla\Brawlhalla.exe", None), "Brawlhalla");
        assert_eq!(
            game_name("/Users/a/Library/Application Support/Steam/steamapps/common/Euro Truck Simulator 2/Euro Truck Simulator 2.app/Contents/MacOS/eurotrucks2", Some("eurotrucks2")),
            "Euro Truck Simulator 2"
        );
        assert_eq!(game_name("/Applications/Chess.app/Contents/MacOS/Chess", Some("Chess")), "Chess");
        assert_eq!(game_name(r"C:\Games\Wow.exe", None), "Wow");
    }

    #[test]
    fn games_by_store_library() {
        assert!(is_game(r"C:\Program Files (x86)\Steam\steamapps\common\Hades\x64\Hades.exe", None, &[]));
        assert!(is_game(r"D:\Epic Games\Fortnite\FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe", None, &[]));
        assert!(is_game("/Users/a/Library/Application Support/Steam/steamapps/common/Stardew Valley/Contents/MacOS/Stardew Valley", None, &[]));
        assert!(is_game(r"C:\Riot Games\VALORANT\live\VALORANT.exe", None, &[]));
    }

    #[test]
    fn launchers_and_apps_are_not_games() {
        assert!(!is_game(r"C:\Program Files (x86)\Steam\steam.exe", None, &[]));
        assert!(!is_game(r"C:\Riot Games\Riot Client\RiotClientServices.exe", None, &[]));
        assert!(!is_game("/Applications/Safari.app/Contents/MacOS/Safari", Some("public.app-category.productivity"), &[]));
        assert!(!is_game(r"C:\Program Files\Google\Chrome\Application\chrome.exe", None, &[]));
    }

    #[test]
    fn declared_and_known_games() {
        assert!(is_game("/Applications/Chess.app/Contents/MacOS/Chess", Some("public.app-category.board-games"), &[]));
        assert!(is_game("/Applications/Celeste.app/Contents/MacOS/Celeste", Some("public.app-category.games"), &[]));
        let known = vec![r"C:\Program Files (x86)\World of Warcraft\_retail_\Wow.exe".to_string()];
        assert!(is_game(r"c:\program files (x86)\world of warcraft\_retail_\wow.exe", None, &known));
        assert!(is_game("/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin/wine64-preloader", None, &[]));
    }
}
