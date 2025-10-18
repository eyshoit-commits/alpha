use anyhow::Result;

use cave_daemon::telemetry;

#[tokio::main]
async fn main() -> Result<()> {
    let _telemetry = telemetry::init("cave-daemon")?;
    cave_daemon::server::run().await
}
