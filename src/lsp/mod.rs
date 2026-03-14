mod analysis;
mod convert;
mod handlers;
mod server;
mod state;

pub fn run_lsp() -> Result<(), String> {
    server::run_stdio_server()
}
