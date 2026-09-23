//! Clipboard writes are explicit user actions. Windows needs no helper process.
#[cfg(not(test))]
pub(crate) fn copy(text: &str) -> Result<(), String> {
    platform::copy(text)
}

// UI tests must not overwrite the user's real clipboard.
#[cfg(test)]
thread_local! {
    pub(crate) static COPIED: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}
#[cfg(test)]
pub(crate) fn copy(text: &str) -> Result<(), String> {
    COPIED.with(|c| *c.borrow_mut() = text.into());
    Ok(())
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, io, ptr};
    type Handle = *mut c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: Handle,
            menu: Handle,
            instance: Handle,
            param: Handle,
        ) -> Handle;
        fn DestroyWindow(window: Handle) -> i32;
        fn OpenClipboard(window: Handle) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, memory: Handle) -> Handle;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
        fn GlobalLock(memory: Handle) -> Handle;
        fn GlobalUnlock(memory: Handle) -> i32;
        fn GlobalFree(memory: Handle) -> Handle;
    }
    struct Window(Handle);
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    struct Memory(Handle);
    impl Drop for Memory {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    GlobalFree(self.0);
                }
            }
        }
    }
    struct Clipboard;
    impl Drop for Clipboard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    fn error(action: &str) -> String {
        format!("{action}: {}", io::Error::last_os_error())
    }

    // Also exercised by an explicitly requested, ignored native clipboard test.
    #[cfg_attr(test, allow(dead_code))]
    pub(super) fn copy(text: &str) -> Result<(), String> {
        if text.contains('\0') {
            return Err("Cannot copy text containing NUL".into());
        }
        let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // A message-only window provides an owner even under ConPTY / VS Code.
        // Passing NULL to OpenClipboard followed by EmptyClipboard leaves no owner.
        let window = Window(unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                -3isize as Handle,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        });
        if window.0.is_null() {
            return Err(error("Create clipboard owner"));
        }
        let mut memory = Memory(unsafe { GlobalAlloc(0x0002, wide.len() * 2) }); // GMEM_MOVEABLE
        if memory.0.is_null() {
            return Err(error("Allocate clipboard text"));
        }
        let data = unsafe { GlobalLock(memory.0) } as *mut u16;
        if data.is_null() {
            return Err(error("Lock clipboard text"));
        }
        unsafe {
            ptr::copy_nonoverlapping(wide.as_ptr(), data, wide.len());
            GlobalUnlock(memory.0);
        }
        let mut opened = false;
        for _ in 0..5 {
            if unsafe { OpenClipboard(window.0) } != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !opened {
            return Err(error("Open clipboard"));
        }
        let _clipboard = Clipboard;
        if unsafe { EmptyClipboard() } == 0 {
            return Err(error("Empty clipboard"));
        }
        if unsafe { SetClipboardData(13, memory.0) }.is_null() {
            // CF_UNICODETEXT
            return Err(error("Write clipboard"));
        }
        memory.0 = ptr::null_mut(); // Ownership transferred to Windows.
        Ok(())
    }

    #[test]
    #[ignore = "writes the Windows clipboard; run explicitly with a clipboard backup"]
    fn native_unicode_clipboard() {
        copy("uart_cnt\t中文\nplatform.cpu_num").unwrap();
    }
}

#[cfg(all(not(windows), not(test)))]
mod platform {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    pub(super) fn copy(text: &str) -> Result<(), String> {
        // Optional desktop helpers; no shell interpolation of source text.
        let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
            &[("pbcopy", &[])]
        } else {
            &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])]
        };
        for (program, args) in candidates {
            let Ok(mut child) = Command::new(program)
                .args(*args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                continue;
            };
            let write = child.stdin.take().unwrap().write_all(text.as_bytes());
            if write.is_ok() && child.wait().is_ok_and(|s| s.success()) {
                return Ok(());
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        Err(
            "Clipboard unavailable (requires pbcopy, wl-copy or xclip); Add to Watch still works"
                .into(),
        )
    }
}
