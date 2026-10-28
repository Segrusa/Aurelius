//! Notification-area icon for the background service (Windows, B24). It shows
//! that Aurelius keeps working after the window is closed, and offers
//! "Open Aurelius" (also on double-click) and "Quit background service".
//! Only the real service shows it; tests and embedded runs don't.

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

    fn open_app() {
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
            let _ = std::process::Command::new(dir.join("aurelius.exe")).current_dir(&dir).spawn();
        }
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
                OPEN => open_app(),
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
                    WM_LBUTTONDBLCLK => open_app(),
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

#[cfg(not(windows))]
mod imp {
    pub fn start(_: String, _: Box<dyn Fn() + Send + Sync>) {}
    pub fn stop() {}
}

pub use imp::{start, stop};
