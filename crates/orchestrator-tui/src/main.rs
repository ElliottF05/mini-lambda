mod app;
mod ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let orchestrator = std::env::args().nth(1).unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    ui::run(orchestrator).await?;
    Ok(())
}