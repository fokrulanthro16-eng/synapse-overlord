mod agent;
mod builder;
mod enhancer;
mod database;
mod gitops;
mod models;
mod rag;
mod safety;
mod sandbox;
mod settings;
mod system;
mod tui;
mod web;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("web") => web::run_web_server().await,
        _ => {
            tui::run_tui()?;
            Ok(())
        }
    }
}
