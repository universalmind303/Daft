use std::sync::{atomic::AtomicBool, Mutex};

use tracing_chrome::ChromeLayerBuilder;
use tracing_subscriber::layer::SubscriberExt;

static TRACING_INIT: AtomicBool = AtomicBool::new(false);

use std::sync::LazyLock;

static CHROME_GUARD_HANDLE: LazyLock<Mutex<Option<tracing_chrome::FlushGuard>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn init_tracing(enable_chrome_trace: bool) {
    use std::sync::atomic::Ordering;

    assert!(
        !TRACING_INIT.swap(true, Ordering::Relaxed),
        "Cannot init tracing, already initialized!"
    );

    if !enable_chrome_trace {
        return; // Do nothing for now
    }

    let mut mg = CHROME_GUARD_HANDLE.lock().unwrap();
    assert!(
        mg.is_none(),
        "Expected chrome flush guard to be None on init"
    );

    let (chrome_layer, guard) = ChromeLayerBuilder::new()
        .trace_style(tracing_chrome::TraceStyle::Threaded)
        .name_fn(Box::new(|event_or_span| {
            match event_or_span {
                tracing_chrome::EventOrSpan::Event(ev) => ev.metadata().name().into(),
                tracing_chrome::EventOrSpan::Span(s) => {
                    // TODO: this is where we should extract out fields (such as node id to show the different pipelines)
                    s.name().into()
                }
            }
        }))
        .build();

    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(chrome_layer))
        .unwrap();

    *mg = Some(guard);
}

pub fn refresh_chrome_trace() -> bool {
    let mut mg = CHROME_GUARD_HANDLE.lock().unwrap();
    if let Some(fg) = mg.as_mut() {
        fg.start_new(None);
        true
    } else {
        false
    }
}
