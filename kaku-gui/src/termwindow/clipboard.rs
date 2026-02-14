use crate::termwindow::TermWindowNotif;
use crate::TermWindow;
use config::keyassignment::{ClipboardCopyDestination, ClipboardPasteSource};
use mux::pane::Pane;
use mux::Mux;
use smol::Timer;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use window::{Clipboard, ClipboardData, WindowOps};

impl TermWindow {
    pub fn copy_to_clipboard(&self, clipboard: ClipboardCopyDestination, text: String) {
        let clipboard = match clipboard {
            ClipboardCopyDestination::Clipboard => [Some(Clipboard::Clipboard), None],
            ClipboardCopyDestination::PrimarySelection => [Some(Clipboard::PrimarySelection), None],
            ClipboardCopyDestination::ClipboardAndPrimarySelection => [
                Some(Clipboard::Clipboard),
                Some(Clipboard::PrimarySelection),
            ],
        };
        for &c in &clipboard {
            if let Some(c) = c {
                self.window.as_ref().unwrap().set_clipboard(c, text.clone());
            }
        }
    }

    /// Show "Copied!" toast notification (disappears after 1.5 seconds)
    pub fn show_copy_toast(&mut self) {
        let now = Instant::now();
        self.copy_toast_at = Some(now);
        if let Some(window) = self.window.clone() {
            let win = window.clone();
            // Trigger fade-out after 1000ms
            let fade_win = win.clone();
            promise::spawn::spawn(async move {
                Timer::after(Duration::from_millis(1000)).await;
                fade_win.invalidate();
            })
            .detach();
            // Clear after 1500ms
            promise::spawn::spawn(async move {
                Timer::after(Duration::from_millis(1500)).await;
                window.notify(TermWindowNotif::Apply(Box::new(move |tw| {
                    if tw.copy_toast_at == Some(now) {
                        tw.copy_toast_at = None;
                    }
                    win.invalidate();
                })));
            })
            .detach();
        }
        if let Some(window) = self.window.as_ref() {
            window.invalidate();
        }
    }

    pub fn paste_from_clipboard(&mut self, pane: &Arc<dyn Pane>, clipboard: ClipboardPasteSource) {
        let pane_id = pane.pane_id();
        log::trace!(
            "paste_from_clipboard in pane {} {:?}",
            pane.pane_id(),
            clipboard
        );
        let window = self.window.as_ref().unwrap().clone();
        let clipboard = match clipboard {
            ClipboardPasteSource::Clipboard => Clipboard::Clipboard,
            ClipboardPasteSource::PrimarySelection => Clipboard::PrimarySelection,
        };
        let quote_dropped_files = self.config.quote_dropped_files;
        let future = window.get_clipboard_data(clipboard);
        promise::spawn::spawn(async move {
            match future.await {
                Ok(data) => {
                    window.notify(TermWindowNotif::Apply(Box::new(move |myself| {
                        let clip = match data_to_paste_string(data, quote_dropped_files) {
                            Some(clip) => clip,
                            None => return,
                        };

                        if let Some(pane) = myself
                            .pane_state(pane_id)
                            .overlay
                            .as_ref()
                            .map(|overlay| overlay.pane.clone())
                            .or_else(|| {
                                let mux = Mux::get();
                                mux.get_pane(pane_id)
                            })
                        {
                            if let Err(err) = pane.send_paste(&clip) {
                                log::warn!("failed to paste clipboard content into pane {pane_id}: {err:#}");
                            }
                        }
                    })));
                }
                Err(err) => {
                    log::warn!("failed to read clipboard for pane {pane_id}: {err:#}");
                }
            }
        })
        .detach();
        self.maybe_scroll_to_bottom_for_input(&pane);
    }
}

fn data_to_paste_string(
    data: ClipboardData,
    quote_dropped_files: config::DroppedFileQuoting,
) -> Option<String> {
    match data {
        ClipboardData::Text(text) => Some(text),
        ClipboardData::Files(paths) => {
            if paths.is_empty() {
                return None;
            }
            Some(format_dropped_paths(paths, quote_dropped_files))
        }
    }
}

fn format_dropped_paths(
    paths: Vec<PathBuf>,
    quote_dropped_files: config::DroppedFileQuoting,
) -> String {
    paths
        .iter()
        .map(|path| quote_path_for_clipboard_paste(path, quote_dropped_files))
        .collect::<Vec<_>>()
        .join(" ")
        + " "
}

fn quote_path_for_clipboard_paste(
    path: &PathBuf,
    quote_dropped_files: config::DroppedFileQuoting,
) -> String {
    let path = path.to_string_lossy();
    match quote_dropped_files {
        config::DroppedFileQuoting::None => path.into_owned(),
        // Clipboard file paste used to be POSIX-quoted before image support was added.
        // Keep that safety baseline for default SpacesOnly mode.
        config::DroppedFileQuoting::SpacesOnly | config::DroppedFileQuoting::Posix => {
            shlex::try_quote(path.as_ref())
                .unwrap_or_else(|_| "".into())
                .into_owned()
        }
        config::DroppedFileQuoting::Windows | config::DroppedFileQuoting::WindowsAlwaysQuoted => {
            quote_dropped_files.escape(path.as_ref())
        }
    }
}
