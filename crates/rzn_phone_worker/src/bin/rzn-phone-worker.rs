#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rzn_phone_worker::run_worker_stdio().await
}
