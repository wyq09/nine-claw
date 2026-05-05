//! macOS：系统原生 `NSTextView`（支持 Fn / 豆包等在网页内受限的听写）。
//! 以 NSAlert 附层面板呈现，插入后通过 `macos-native-composer-insert` 发回 Web 层。
//!
//! 若后续要下掉该方案：删除本模块、`Cargo.toml` 中 macOS 依赖、
//! `lib.rs` 中的 mod/command、前端按钮与 `ChatWorkspace` 内 listen 即可。

#[cfg(target_os = "macos")]
mod macos_impl {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2::MainThreadOnly;
    use objc2_app_kit::{
        NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSModalResponse, NSScrollView, NSTextView,
        NSWindow,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use tauri::{AppHandle, Emitter, Manager};

    fn nsstring_to_string(s: &NSString) -> String {
        unsafe {
            let ptr = s.UTF8String();
            if ptr.is_null() {
                return String::new();
            }
            std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }

    fn open_on_main(app: &AppHandle) -> Result<(), String> {
        let mtm = MainThreadMarker::new().ok_or_else(|| "需要主线程".to_string())?;

        let parent_handle = app
            .get_webview_window("main")
            .ok_or_else(|| "未找到主窗口".to_string())?;
        let parent_ptr = parent_handle.ns_window().map_err(|e| e.to_string())?;
        if parent_ptr.is_null() {
            return Err("主窗口句柄无效".to_string());
        }
        let parent = unsafe { &*(parent_ptr.cast::<NSWindow>()) };

        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str("系统原生语音输入"));
        alert.setInformativeText(&NSString::from_str(
            "在下方文本框内可使用 Fn 听写、豆包等系统级输入法。完成后点「插入到聊天」；取消则丢弃。",
        ));
        alert.setAlertStyle(NSAlertStyle::Informational);

        let scroll_frame = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: 460.0,
                height: 180.0,
            },
        };
        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), scroll_frame);
        scroll.setHasVerticalScroller(true);
        scroll.setHasHorizontalScroller(false);
        scroll.setAutohidesScrollers(true);

        let tv_frame = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: 440.0,
                height: 400.0,
            },
        };
        let text_view = NSTextView::initWithFrame(NSTextView::alloc(mtm), tv_frame);
        scroll.setDocumentView(Some(&text_view));

        alert.setAccessoryView(Some(&scroll));
        alert.addButtonWithTitle(&NSString::from_str("插入到聊天"));
        alert.addButtonWithTitle(&NSString::from_str("取消"));

        let text_retained: Retained<NSTextView> = Retained::clone(&text_view);
        let app_for_block = app.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            if response == NSAlertFirstButtonReturn {
                let text = nsstring_to_string(&text_retained.string());
                if has_meaningful_text(&text) {
                    let _ = app_for_block.emit(
                        "macos-native-composer-insert",
                        serde_json::json!({ "text": text }),
                    );
                }
            }
        });

        alert.beginSheetModalForWindow_completionHandler(parent, Some(&*handler));
        Ok(())
    }

    fn has_meaningful_text(s: &str) -> bool {
        !s.trim().is_empty()
    }

    pub fn open(app: &AppHandle) -> Result<(), String> {
        let handle = app.clone();
        let for_closure = handle.clone();
        handle
            .run_on_main_thread(move || {
                if let Err(err) = open_on_main(&for_closure) {
                    log::warn!("macos native dictation panel: {err}");
                }
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "macos")]
pub use macos_impl::open;

#[cfg(not(target_os = "macos"))]
pub fn open(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("仅 macOS 支持系统原生语音面板".to_string())
}
