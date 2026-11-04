//! Notification-area icon for the background service: Windows (B24), the status
//! bar on Linux (StatusNotifierItem) and the menu bar on macOS. It shows that
//! Aurelius keeps working after the window is closed, and offers "Open Aurelius"
//! and "Quit background service". Only the real service shows it; tests and
//! embedded runs don't.

/// Start the desktop app from the folder this service is installed in.
#[cfg(windows)]
fn open_app() {
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        let program = crate::updater::app_program(&dir);
        let folder = program.parent().map_or_else(|| dir.clone(), std::path::Path::to_path_buf);
        let _ = std::process::Command::new(program).current_dir(folder).spawn();
    }
}

/// Start the desktop app: `app/aurelius` beside this service (`scripts/install-linux.sh`).
#[cfg(target_os = "linux")]
fn open_app() {
    let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) else { return };
    let folder = dir.join("app");
    let program = folder.join("aurelius");
    if let Err(err) = std::process::Command::new(&program).current_dir(&folder).spawn() {
        tracing::warn!(error = %err, "could not open the app");
    }
}

/// Start the desktop app, wherever Launch Services knows it from.
#[cfg(target_os = "macos")]
fn open_app() {
    if let Err(err) = std::process::Command::new("open").args(["-a", "Aurelius"]).spawn() {
        tracing::warn!(error = %err, "could not open the app");
    }
}

/// The logo's polygon in its 120×120 box, the same as `scripts/make-icon.py`.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
const LOGO: [(f32, f32); 6] = [(60., 14.), (104., 106.), (82., 106.), (60., 56.), (38., 106.), (16., 106.)];

/// The logo as RGBA pixels: white on its green tile, or (a macOS template image) black on nothing.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn logo_rgba(size: u32, tile: bool) -> Vec<u8> {
    const SAMPLES: u32 = 4;
    const GREEN: [f32; 3] = [0x1f as f32, 0x4a as f32, 0x33 as f32];
    let in_logo = |x: f32, y: f32| {
        // Even-odd rule, on the polygon scaled about the middle of the box.
        let scale = if tile { 0.72 } else { 1.0 };
        let (x, y) = ((x - 60.) / scale + 60., (y - 60.) / scale + 60.);
        let mut inside = false;
        for i in 0..LOGO.len() {
            let (ax, ay) = LOGO[i];
            let (bx, by) = LOGO[(i + 1) % LOGO.len()];
            if (ay > y) != (by > y) && x < (bx - ax) * (y - ay) / (by - ay) + ax {
                inside = !inside;
            }
        }
        inside
    };
    let in_tile = |x: f32, y: f32| {
        let radius = 28.;
        let dx = (radius - x).max(x - (120. - radius)).max(0.);
        let dy = (radius - y).max(y - (120. - radius)).max(0.);
        dx * dx + dy * dy <= radius * radius
    };
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut mark, mut back) = (0u32, 0u32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * 120. / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * 120. / size as f32;
                    if in_logo(x, y) {
                        mark += 1;
                    } else if tile && in_tile(x, y) {
                        back += 1;
                    }
                }
            }
            let covered = (mark + back) as f32;
            if covered == 0. {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let white = if tile { 255. } else { 0. };
            let part = |green: f32| ((white * mark as f32 + green * back as f32) / covered).round() as u8;
            out.extend_from_slice(&[part(GREEN[0]), part(GREEN[1]), part(GREEN[2])]);
            out.push((covered * 255. / (SAMPLES * SAMPLES) as f32).round() as u8);
        }
    }
    out
}

/// Runs the service. On macOS the status item needs the main thread for its
/// event loop, so the service goes on another thread there and this never returns.
#[cfg(not(target_os = "macos"))]
pub fn run_service_thread<E>(work: impl FnOnce() -> Result<(), E>) -> Result<(), E> {
    work()
}

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW, GetCursorPos, GetMessageW,
        IDI_APPLICATION, LoadIconW, MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
        SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY,
        WM_LBUTTONDBLCLK, WM_RBUTTONUP, WNDCLASSW,
    };

    const CALLBACK: u32 = WM_APP + 1;
    const OPEN: usize = 1;
    const QUIT: usize = 2;

    static WINDOW: AtomicIsize = AtomicIsize::new(0);
    static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
    static TIP: OnceLock<String> = OnceLock::new();
    static ON_QUIT: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
        // SAFETY: plain C struct; zero is a valid starting state.
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = 1;
        data
    }

    unsafe fn add_icon(hwnd: HWND) {
        let mut data = icon_data(hwnd);
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = CALLBACK;
        // SAFETY: icon resource 1 is embedded by build.rs; fall back to the stock icon.
        unsafe {
            let module = GetModuleHandleW(std::ptr::null());
            data.hIcon = LoadIconW(module, 1 as _);
            if data.hIcon.is_null() {
                data.hIcon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
            }
        }
        let tip: Vec<u16> = TIP.get().map(String::as_str).unwrap_or("Aurelius").encode_utf16().take(127).collect();
        data.szTip[..tip.len()].copy_from_slice(&tip);
        // SAFETY: `data` is fully initialised for NIM_ADD.
        unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
    }

    unsafe fn menu(hwnd: HWND) {
        // SAFETY: standard popup-menu sequence on the tray window's own thread.
        unsafe {
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, OPEN, wide("Open Aurelius").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, QUIT, wide("Quit background service").as_ptr());
            let mut at = POINT { x: 0, y: 0 };
            GetCursorPos(&mut at);
            // Required so the menu closes when clicking elsewhere.
            SetForegroundWindow(hwnd);
            let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, at.x, at.y, 0, hwnd, std::ptr::null()) as usize;
            DestroyMenu(menu);
            match chosen {
                OPEN => super::open_app(),
                QUIT => {
                    if let Some(quit) = ON_QUIT.get() {
                        quit();
                    }
                }
                _ => {}
            }
        }
    }

    unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: called by Windows on the tray thread with a valid window.
        unsafe {
            match msg {
                CALLBACK => match (lparam & 0xffff) as u32 {
                    WM_LBUTTONDBLCLK => super::open_app(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => menu(hwnd),
                    _ => {}
                },
                WM_DESTROY => {
                    Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
                    PostQuitMessage(0);
                }
                // Explorer restarted: put the icon back.
                m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => add_icon(hwnd),
                _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
            }
            0
        }
    }

    pub fn start(tip: String, on_quit: Box<dyn Fn() + Send + Sync>) {
        let _ = TIP.set(tip);
        let _ = ON_QUIT.set(on_quit);
        let spawned = std::thread::Builder::new().name("aurelius-tray".into()).spawn(|| {
            // SAFETY: window class, window and message loop all live on this thread.
            unsafe {
                let class = wide("AureliusTray");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: GetModuleHandleW(std::ptr::null()),
                    lpszClassName: class.as_ptr(),
                    // SAFETY: remaining fields may be zero.
                    ..std::mem::zeroed()
                };
                RegisterClassW(&wc);
                TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);
                let hwnd = CreateWindowExW(0, class.as_ptr(), wide("Aurelius").as_ptr(), 0, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), wc.hInstance, std::ptr::null());
                if hwnd.is_null() {
                    tracing::warn!("no tray icon: window creation failed");
                    return;
                }
                WINDOW.store(hwnd as isize, Ordering::Relaxed);
                add_icon(hwnd);
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
        if let Err(err) = spawned {
            tracing::warn!(error = %err, "no tray icon");
        }
    }

    /// Remove the icon (service stopping), so no stale icon stays behind.
    pub fn stop() {
        let hwnd = WINDOW.swap(0, Ordering::Relaxed);
        if hwnd != 0 {
            // SAFETY: the window belongs to the tray thread; its default WM_CLOSE destroys it there (WM_DESTROY removes the icon).
            unsafe { PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0) };
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use ksni::blocking::TrayMethods;
    use std::sync::Mutex;

    struct Tray {
        tip: String,
        on_quit: Box<dyn Fn() + Send + Sync>,
    }

    impl ksni::Tray for Tray {
        fn id(&self) -> String {
            "aurelius-daemon".into()
        }
        fn title(&self) -> String {
            "Aurelius".into()
        }
        fn category(&self) -> ksni::Category {
            ksni::Category::ApplicationStatus
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            [22, 32, 48, 64]
                .into_iter()
                .map(|size| {
                    // The protocol wants ARGB in network byte order.
                    let mut data = super::logo_rgba(size as u32, true);
                    for pixel in data.chunks_exact_mut(4) {
                        pixel.rotate_right(1);
                    }
                    ksni::Icon { width: size, height: size, data }
                })
                .collect()
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip { title: self.tip.clone(), ..Default::default() }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            super::open_app();
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            vec![
                StandardItem { label: "Open Aurelius".into(), activate: Box::new(|_: &mut Self| super::open_app()), ..Default::default() }.into(),
                ksni::MenuItem::Separator,
                StandardItem { label: "Quit background service".into(), activate: Box::new(|tray: &mut Self| (tray.on_quit)()), ..Default::default() }.into(),
            ]
        }
    }

    static HANDLE: Mutex<Option<ksni::blocking::Handle<Tray>>> = Mutex::new(None);

    pub fn start(tip: String, on_quit: Box<dyn Fn() + Send + Sync>) {
        // Its own thread: this blocks, and the caller is on the service's runtime. A status
        // bar that starts after the service still gets the icon (`assume_sni_available`).
        let spawned = std::thread::Builder::new().name("aurelius-tray".into()).spawn(move || {
            match (Tray { tip, on_quit }).assume_sni_available(true).spawn() {
                Ok(handle) => *HANDLE.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle),
                Err(err) => tracing::warn!(error = %err, "no status bar icon"),
            }
        });
        if let Err(err) = spawned {
            tracing::warn!(error = %err, "no status bar icon");
        }
    }

    /// Remove the icon (service stopping), so no stale icon stays behind.
    pub fn stop() {
        if let Some(handle) = HANDLE.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = std::thread::spawn(move || {
                let _ = handle.shutdown().wait();
            })
            .join();
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIconBuilder};

    type OnQuit = Box<dyn Fn() + Send + Sync>;
    static ASKED: Mutex<Option<(String, OnQuit)>> = Mutex::new(None);
    static STOP: AtomicBool = AtomicBool::new(false);

    pub fn start(tip: String, on_quit: OnQuit) {
        *ASKED.lock().unwrap_or_else(|e| e.into_inner()) = Some((tip, on_quit));
    }

    /// Remove the icon (service stopping), so no stale icon stays behind.
    pub fn stop() {
        STOP.store(true, Ordering::Relaxed);
    }

    fn build(tip: &str) -> Option<(tray_icon::TrayIcon, tray_icon::menu::MenuId, tray_icon::menu::MenuId)> {
        const SIZE: u32 = 44;
        let open = MenuItem::new("Open Aurelius", true, None);
        let quit = MenuItem::new("Quit background service", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).ok()?;
        let icon = Icon::from_rgba(super::logo_rgba(SIZE, false), SIZE, SIZE).ok()?;
        let tray = TrayIconBuilder::new().with_icon_templated(icon).with_tooltip(tip).with_menu(Box::new(menu)).build().ok()?;
        Some((tray, open.id().clone(), quit.id().clone()))
    }

    /// The event loop on the main thread; the service runs beside it and ends the process.
    pub fn run_service_thread<E: std::fmt::Display>(work: impl FnOnce() -> Result<(), E> + Send + 'static) -> Result<(), E> {
        if std::env::var_os("AURELIUS_NO_TRAY").is_some() {
            return work();
        }
        // As big as the main thread's stack, which this work would otherwise have had.
        let service = std::thread::Builder::new().name("aurelius-service".into()).stack_size(8 * 1024 * 1024).spawn(move || {
            let code = match work() {
                Ok(()) => 0,
                Err(err) => {
                    eprintln!("Error: {err}");
                    1
                }
            };
            std::process::exit(code);
        });
        if let Err(err) = service {
            tracing::error!(error = %err, "the service thread did not start");
            std::process::exit(1);
        }
        let mut event_loop = EventLoopBuilder::new().build();
        // A background service: no Dock icon, no menu bar of its own.
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        let mut shown = None;
        let mut on_quit: Option<OnQuit> = None;
        event_loop.run(move |_, _, flow| {
            *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(200));
            if STOP.load(Ordering::Relaxed) {
                shown = None;
            } else if let Some((tip, quit)) = ASKED.lock().unwrap_or_else(|e| e.into_inner()).take() {
                shown = build(&tip);
                on_quit = Some(quit);
            }
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                let Some((_, open, quit)) = &shown else { continue };
                if event.id == *open {
                    super::open_app();
                } else if event.id == *quit
                    && let Some(quit) = &on_quit
                {
                    quit();
                }
            }
        })
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    pub fn start(_: String, _: Box<dyn Fn() + Send + Sync>) {}
    pub fn stop() {}
}

pub use imp::{start, stop};

#[cfg(target_os = "macos")]
pub use imp::run_service_thread;

#[cfg(test)]
mod tests {
    use super::logo_rgba;

    #[test]
    fn the_logo_is_white_on_green_and_round_at_the_corners() {
        let size = 64usize;
        let data = logo_rgba(size as u32, true);
        assert_eq!(data.len(), size * size * 4);
        let at = |x: usize, y: usize| &data[(y * size + x) * 4..(y * size + x) * 4 + 4];
        // The very corner is cut off by the tile's roundness.
        assert_eq!(at(0, 0)[3], 0);
        // A plain part of the tile, and the leg of the A.
        assert_eq!(at(5, 32), &[0x1f, 0x4a, 0x33, 255]);
        assert_eq!(at(32, 18), &[255, 255, 255, 255]);
    }

    #[test]
    fn the_template_image_is_only_the_mark() {
        let data = logo_rgba(44, false);
        assert!(data.chunks_exact(4).all(|p| p[..3] == [0, 0, 0]));
        assert!(data.chunks_exact(4).any(|p| p[3] == 255));
        assert_eq!(data[3], 0);
    }
}
