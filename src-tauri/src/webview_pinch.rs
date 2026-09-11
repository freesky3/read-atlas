use tauri::Manager;
#[cfg(windows)]
use tauri::{AppHandle, Emitter};

/// WebView2 swallows trackpad pinch when `zoomHotkeysEnabled` is false
/// (`IsPinchZoomEnabled` is tied to that flag in wry). Re-enable pinch, keep
/// the page zoom factor at 1, and forward the native scale to the reader.
pub fn install(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.set_zoom(1.0);

    #[cfg(windows)]
    {
        let handle = app.handle().clone();
        let _ = window.with_webview(move |webview| {
            let _ = install_windows(webview, handle);
        });
    }
}

#[cfg(windows)]
fn install_windows(
    webview: tauri::webview::PlatformWebview,
    app: AppHandle,
) -> windows::core::Result<()> {
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::ICoreWebView2Settings5, ZoomFactorChangedEventHandler,
    };
    use windows::core::Interface;

    unsafe {
        let controller = webview.controller();
        if let Ok(core) = controller.CoreWebView2() {
            if let Ok(settings) = core.Settings() {
                if let Ok(settings5) = settings.cast::<ICoreWebView2Settings5>() {
                    let _ = settings5.SetIsPinchZoomEnabled(true);
                }
            }
        }

        let watched = controller.clone();
        let mut token = 0_i64;
        controller.add_ZoomFactorChanged(
            &ZoomFactorChangedEventHandler::create(Box::new(move |_, _| {
                let mut factor = 1.0;
                watched.ZoomFactor(&mut factor)?;
                if (factor - 1.0).abs() > 0.000_5 {
                    watched.SetZoomFactor(1.0)?;
                    let _ = app.emit("webview-pinch-zoom", factor);
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}
