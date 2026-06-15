mod detect;
mod net;
mod nms;
#[cfg(windows)]
mod prompt;

use detect::Detector;

/// Resolve the worker name when none was passed on the command line.
/// On Windows this pops a native modal; elsewhere it falls back to "worker".
#[cfg(windows)]
fn resolve_name_interactive() -> String {
    prompt::ask_worker_name().unwrap_or_else(|| "worker".to_string())
}

#[cfg(not(windows))]
fn resolve_name_interactive() -> String {
    "worker".to_string()
}

#[tokio::main]
async fn main() -> ! {
    // name is the second argument; if absent, ask interactively (Windows modal).
    let args: Vec<String> = std::env::args().collect();
    let name = match args.get(1).cloned() {
        Some(n) => n,
        None => resolve_name_interactive(),
    };

    loop {
        if let Err(e) = net::main_loop(&name).await {
            println!("Error in main loop: {:?}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
