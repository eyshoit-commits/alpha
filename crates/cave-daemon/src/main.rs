use cave_daemon::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    server::run().await
}
