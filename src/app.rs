//! The desktop window.
//!
//! One `tao` window hosting one `wry` webview. Everything the page needs is
//! served from the [`mdview` protocol](crate::protocol), so relative links and
//! images inside a document resolve naturally and clicking through to another
//! `.md` file is ordinary navigation rather than a special case.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu, accelerator::Accelerator};
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::http::{Request, Response, header};
use wry::{DragDropEvent, WebViewBuilder};

use crate::document::{Assets, Document, Theme};
use crate::markdown::RenderOptions;
use crate::protocol::{self, Resolved};
use crate::watch::FileWatcher;

/// How the viewer was asked to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Document to open, if one was named.
    pub path: Option<PathBuf>,
    /// Colour scheme preference.
    pub theme: Theme,
    /// Reload automatically when the file changes on disk.
    pub watch: bool,
    /// Render Mermaid diagrams.
    pub mermaid: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            path: None,
            theme: Theme::Auto,
            watch: true,
            mermaid: true,
        }
    }
}

/// Mutable state shared between the event loop and the webview's callbacks.
#[derive(Debug)]
struct State {
    /// Document currently displayed.
    path: Option<PathBuf>,
    /// Colour scheme preference.
    theme: Theme,
    /// Options handed to the renderer.
    render: RenderOptions,
}

impl State {
    /// Render `path`, or the welcome page when there is nothing to show.
    fn page(&self, path: Option<&Path>) -> String {
        let document = match path {
            None => Document::welcome(self.theme),
            Some(path) => match std::fs::read_to_string(path) {
                Ok(source) => Document::from_markdown(
                    &source,
                    &self.render,
                    self.theme,
                    &path.to_string_lossy(),
                ),
                Err(err) => Document::error(&format!("{}: {err}", path.display()), self.theme),
            },
        };
        document.to_html(Assets::Linked)
    }
}

/// Things that make the event loop do something.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    /// Show a different document.
    Open(PathBuf),
    /// Ask the user to pick a file.
    Prompt,
    /// Re-read the current document.
    Reload,
    /// Switch between light and dark.
    ToggleTheme,
    /// Adjust the text size: -1, +1, or 0 to reset.
    Zoom(i32),
    /// Close the window.
    Quit,
    /// The page navigated itself somewhere; follow along.
    Navigated(PathBuf),
}

impl Action {
    /// Parse a message posted by the page.
    fn from_ipc(message: &str) -> Option<Self> {
        match message {
            "open" => Some(Action::Prompt),
            "reload" => Some(Action::Reload),
            "quit" => Some(Action::Quit),
            "toggle-theme" => Some(Action::ToggleTheme),
            _ => None,
        }
    }

    /// Parse a menu item id.
    fn from_menu_id(id: &str) -> Option<Self> {
        match id {
            "open" => Some(Action::Prompt),
            "reload" => Some(Action::Reload),
            "toggle-theme" => Some(Action::ToggleTheme),
            "zoom-in" => Some(Action::Zoom(1)),
            "zoom-out" => Some(Action::Zoom(-1)),
            "zoom-reset" => Some(Action::Zoom(0)),
            _ => None,
        }
    }
}

/// Open a window and run until it closes.
pub fn run(options: Options) -> Result<(), Box<dyn Error>> {
    let mut render = RenderOptions::viewer();
    render.mermaid = options.mermaid;

    let state = Arc::new(Mutex::new(State {
        path: options.path.clone(),
        theme: options.theme,
        render,
    }));

    let event_loop = EventLoopBuilder::<Action>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title(window_title(options.path.as_deref()))
        .with_inner_size(LogicalSize::new(1000.0, 780.0))
        .with_min_inner_size(LogicalSize::new(360.0, 240.0))
        .build(&event_loop)?;

    let menu = build_menu()?;
    install_menu(&menu, &window)?;
    forward_menu_events(proxy.clone());

    let webview = build_webview(&window, Arc::clone(&state), proxy.clone(), &options)?;

    let mut watcher = watcher(&options, proxy.clone());
    if let Some(path) = options.path.as_deref() {
        watch(&mut watcher, Some(path));
    }

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {}
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,

            Event::UserEvent(action) => match action {
                Action::Quit => *control_flow = ControlFlow::Exit,

                Action::Prompt => {
                    if let Some(path) =
                        prompt_for_file(state.lock().expect("state").path.as_deref())
                    {
                        let _ = proxy.send_event(Action::Open(path));
                    }
                }

                Action::Open(path) => {
                    state.lock().expect("state").path = Some(path.clone());
                    window.set_title(&window_title(Some(&path)));
                    watch(&mut watcher, Some(&path));
                    let _ = webview.load_url(&protocol::document_url(&path));
                }

                Action::Navigated(path) => {
                    // The page followed a link on its own; keep up with it.
                    state.lock().expect("state").path = Some(path.clone());
                    window.set_title(&window_title(Some(&path)));
                    watch(&mut watcher, Some(&path));
                }

                Action::Reload => {
                    let url = {
                        let state = state.lock().expect("state");
                        state.path.as_deref().map(protocol::document_url)
                    };
                    match url {
                        Some(url) => {
                            let _ = webview.load_url(&url);
                        }
                        None => {
                            let _ = webview.evaluate_script("location.reload()");
                        }
                    }
                }

                Action::ToggleTheme => {
                    let theme = {
                        let mut state = state.lock().expect("state");
                        state.theme = state.theme.toggled();
                        state.theme
                    };
                    let _ = webview.evaluate_script(&format!(
                        "document.documentElement.setAttribute('data-theme-preference','{}');\
                         window.mdview&&window.mdview.applyTheme();",
                        theme.as_str()
                    ));
                }

                Action::Zoom(delta) => {
                    let _ = webview
                        .evaluate_script(&format!("window.mdview&&window.mdview.zoom({delta})"));
                }
            },

            _ => {}
        }
    })
}

/// Assemble the webview, wiring up the protocol, IPC, navigation and drag/drop.
fn build_webview(
    window: &tao::window::Window,
    state: Arc<Mutex<State>>,
    proxy: EventLoopProxy<Action>,
    options: &Options,
) -> Result<wry::WebView, wry::Error> {
    let protocol_state = Arc::clone(&state);
    let navigation_proxy = proxy.clone();
    let ipc_proxy = proxy.clone();
    let drop_proxy = proxy;

    let initial_url = match options.path.as_deref() {
        Some(path) => protocol::document_url(path),
        None => format!("{}/{}", protocol::origin(), "__mdview__/welcome"),
    };

    let builder = WebViewBuilder::new()
        .with_custom_protocol(protocol::SCHEME.to_string(), move |_id, request| {
            serve(&protocol_state, &request).map(Into::into)
        })
        .with_ipc_handler(move |request: Request<String>| {
            if let Some(action) = Action::from_ipc(request.body()) {
                let _ = ipc_proxy.send_event(action);
            }
        })
        .with_navigation_handler(move |url: String| on_navigate(&url, &navigation_proxy))
        .with_drag_drop_handler(move |event| match event {
            DragDropEvent::Drop { paths, .. } => {
                match paths.into_iter().find(|p| crate::is_markdown_path(p)) {
                    Some(path) => {
                        let _ = drop_proxy.send_event(Action::Open(path));
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        })
        .with_url(initial_url);

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let webview = builder.build(window)?;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().expect("gtk vbox"))?
    };

    Ok(webview)
}

/// Answer one request on the `mdview` protocol.
fn serve(state: &Arc<Mutex<State>>, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let url = request.uri().to_string();

    // The welcome screen is not backed by a file.
    if protocol::url_path(&url).as_deref() == Some("/__mdview__/welcome") {
        let body = state.lock().expect("state").page(None);
        return html_response(body);
    }

    match protocol::resolve(&url) {
        Resolved::Asset { body, content_type } => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "no-cache")
            .body(body.as_bytes().to_vec())
            .expect("asset response"),

        Resolved::Markdown(path) => {
            let body = state.lock().expect("state").page(Some(&path));
            html_response(body)
        }

        Resolved::File { path, content_type } => match std::fs::read(&path) {
            Ok(bytes) => Response::builder()
                .header(header::CONTENT_TYPE, content_type)
                .body(bytes)
                .expect("file response"),
            Err(err) => text_response(404, format!("{}: {err}", path.display())),
        },

        Resolved::NotFound => text_response(404, "Not found".to_string()),
    }
}

/// A rendered page.
fn html_response(body: String) -> Response<Vec<u8>> {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        // Documents are re-rendered from disk on every load.
        .header(header::CACHE_CONTROL, "no-store")
        .body(body.into_bytes())
        .expect("html response")
}

/// A plain-text error.
fn text_response(status: u16, body: String) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(body.into_bytes())
        .expect("text response")
}

/// Decide what happens when the page tries to navigate.
///
/// Links inside the document tree are followed in place; anything else is handed
/// to the operating system, so a viewer never turns into a browser.
fn on_navigate(url: &str, proxy: &EventLoopProxy<Action>) -> bool {
    if !url.starts_with(protocol::origin()) {
        if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
            let _ = open::that_detached(url);
        }
        return false;
    }

    if let Some(path) = protocol::url_path(url).and_then(|p| protocol::to_fs_path(&p))
        && crate::is_markdown_path(&path)
    {
        let _ = proxy.send_event(Action::Navigated(path));
    }
    true
}

/// Window title: the file name, or just the application name.
fn window_title(path: Option<&Path>) -> String {
    match path.and_then(|p| p.file_name()).and_then(|n| n.to_str()) {
        Some(name) => format!("{name} — mdview"),
        None => "mdview".to_string(),
    }
}

/// Create the file watcher, unless watching was turned off.
fn watcher(options: &Options, proxy: EventLoopProxy<Action>) -> Option<FileWatcher> {
    if !options.watch {
        return None;
    }
    match FileWatcher::new(move || {
        let _ = proxy.send_event(Action::Reload);
    }) {
        Ok(watcher) => Some(watcher),
        Err(err) => {
            eprintln!("mdview: live reload unavailable: {err}");
            None
        }
    }
}

/// Point the watcher at a different file.
fn watch(watcher: &mut Option<FileWatcher>, path: Option<&Path>) {
    if let Some(watcher) = watcher.as_mut()
        && let Err(err) = watcher.watch(path)
    {
        eprintln!("mdview: cannot watch {path:?}: {err}");
    }
}

/// Show a native file picker.
fn prompt_for_file(current: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Open Markdown document")
        .add_filter("Markdown", crate::MARKDOWN_EXTENSIONS)
        .add_filter("All files", &["*"]);

    if let Some(dir) = current.and_then(Path::parent) {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_file()
}

/// Build the application menu.
fn build_menu() -> Result<Menu, muda::Error> {
    let menu = Menu::new();

    #[cfg(target_os = "macos")]
    {
        let app = Submenu::new("mdview", true);
        app.append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])?;
        menu.append(&app)?;
    }

    let file = Submenu::new("&File", true);
    file.append_items(&[
        &MenuItem::with_id("open", "&Open…", true, accelerator("CmdOrCtrl+O")),
        &MenuItem::with_id("reload", "&Reload", true, accelerator("CmdOrCtrl+R")),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::close_window(None),
    ])?;
    menu.append(&file)?;

    // macOS routes copy and select-all through the menu bar, so without these
    // the usual shortcuts never reach the webview. Elsewhere the webview
    // handles them itself and a menu would only duplicate them.
    #[cfg(target_os = "macos")]
    {
        let edit = Submenu::new("&Edit", true);
        edit.append_items(&[
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::select_all(None),
        ])?;
        menu.append(&edit)?;
    }

    let view = Submenu::new("&View", true);
    view.append_items(&[
        &MenuItem::with_id("zoom-in", "Zoom &In", true, accelerator("CmdOrCtrl+Plus")),
        &MenuItem::with_id(
            "zoom-out",
            "Zoom &Out",
            true,
            accelerator("CmdOrCtrl+Minus"),
        ),
        &MenuItem::with_id(
            "zoom-reset",
            "&Actual Size",
            true,
            accelerator("CmdOrCtrl+0"),
        ),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(
            "toggle-theme",
            "Toggle &Dark Mode",
            true,
            accelerator("CmdOrCtrl+D"),
        ),
        &PredefinedMenuItem::fullscreen(None),
    ])?;
    menu.append(&view)?;

    Ok(menu)
}

/// Parse an accelerator, ignoring one the platform cannot express.
fn accelerator(spec: &str) -> Option<Accelerator> {
    spec.parse().ok()
}

/// Attach the menu to the platform's menu bar.
fn install_menu(menu: &Menu, window: &tao::window::Window) -> Result<(), Box<dyn Error>> {
    #[cfg(target_os = "macos")]
    {
        let _ = window;
        menu.init_for_nsapp();
    }
    #[cfg(target_os = "windows")]
    {
        use tao::platform::windows::WindowExtWindows;
        // Safety: the window is alive for as long as the menu is installed.
        unsafe { menu.init_for_hwnd(window.hwnd() as _)? };
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        use tao::platform::unix::WindowExtUnix;
        menu.init_for_gtk_window(window.gtk_window(), window.default_vbox())?;
    }
    Ok(())
}

/// Bridge muda's global menu channel into the event loop.
fn forward_menu_events(proxy: EventLoopProxy<Action>) {
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(action) = Action::from_menu_id(event.id().as_ref()) {
            let _ = proxy.send_event(action);
        }
    }));
}
