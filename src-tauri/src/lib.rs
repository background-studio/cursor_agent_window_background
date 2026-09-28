mod cursor_host;
mod injector;
mod media;
mod models;
mod payload;
mod plugin;
mod plugin_ipc;
mod protocol;
mod worker;

use std::sync::Arc;

use worker::WorkerState;

pub async fn run() -> Result<(), String> {
    let state = Arc::new(WorkerState::load()?);
    worker::start_tick(Arc::clone(&state));
    plugin_ipc::start(Arc::clone(&state));
    loop {
        if state.is_shutting_down() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    Ok(())
}
