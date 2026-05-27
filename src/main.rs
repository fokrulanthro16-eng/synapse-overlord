mod agent;
mod builder;
mod enhancer;
mod database;
mod gitops;
mod llm;
mod models;
mod rag;
mod routes;
mod safety;
mod sandbox;
mod settings;
mod system;
mod tools;
mod tui;
mod web;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();

    // Accept all common spellings for "start the web server":
    //   cargo run -- web
    //   cargo run -- --web
    //   cargo run -- -w
    //   cargo run -- serve
    let web_mode = args
        .get(1)
        .map(|s| matches!(s.as_str(), "web" | "--web" | "-w" | "serve"))
        .unwrap_or(false);

    if web_mode {
        web::run_web_server().await
    } else {
        tui::run_tui()?;
        Ok(())
    }
}
