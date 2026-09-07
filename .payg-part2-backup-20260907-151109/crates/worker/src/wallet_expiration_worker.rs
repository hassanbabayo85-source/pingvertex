use application::ports::WalletRepository;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub struct WalletExpirationWorker {
    wallet_repository: Arc<dyn WalletRepository>,
    interval: Duration,
    batch_size: u32,
}
impl WalletExpirationWorker {
    pub fn new(
        wallet_repository: Arc<dyn WalletRepository>,
        interval: Duration,
        batch_size: u32,
    ) -> Self {
        Self {
            wallet_repository,
            interval,
            batch_size,
        }
    }
    pub async fn run_forever(
        &self,
        cancel: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut ticker = tokio::time::interval(self.interval);
        loop {
            tokio::select! { _=cancel.cancelled()=>return Ok(()), _=ticker.tick()=>{ if let Err(e)=self.wallet_repository.expire_promotional_credits(self.batch_size).await { tracing::error!(error=%e,"promotional credit expiration failed"); } } }
        }
    }
}
