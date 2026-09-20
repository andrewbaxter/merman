//! Displays the served page in a native webview window.
use loga::ResultContext;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

/// Start the event loop, turning gtk's panic on failure to connect to a
/// display into an error suggesting the browser fallback.
fn new_event_loop() -> Result<EventLoop<()>, loga::Error> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| { }));
    let res = std::panic::catch_unwind(|| EventLoop::new());
    std::panic::set_hook(hook);
    return res.map_err(
        |_| loga::err("Error opening a window (is a display available?) - try viewing in a browser with --browser"),
    );
}

/// Open a window showing `url`; doesn't return until the window is closed.
pub fn show(title: &str, url: &str) -> Result<(), loga::Error> {
    let event_loop = new_event_loop()?;
    let window =
        WindowBuilder::new()
            .with_title(title)
            .build(&event_loop)
            .context("Error creating window")?;
    let builder = WebViewBuilder::new().with_url(url);
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android"))]
    let _webview = builder.build(&window).context("Error creating webview")?;
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android")))]
    let _webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;

        let vbox = window.default_vbox().context("Window has no gtk container to put the webview in")?;
        builder.build_gtk(vbox).context("Error creating webview")?
    };
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *control_flow = ControlFlow::Exit;
        }
    });
}
