use application::ports::{NotificationChannelRepository, NotificationService, WalletRepository};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub struct WalletNotificationEmailWorker {
    wallet: Arc<dyn WalletRepository>,
    channels: Arc<dyn NotificationChannelRepository>,
    notifications: Arc<dyn NotificationService>,
    interval: Duration,
}

impl WalletNotificationEmailWorker {
    pub fn new(
        wallet: Arc<dyn WalletRepository>,
        channels: Arc<dyn NotificationChannelRepository>,
        notifications: Arc<dyn NotificationService>,
        interval: Duration,
    ) -> Self {
        Self { wallet, channels, notifications, interval }
    }

    pub async fn run_forever(
        &self,
        cancel: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut ticker=tokio::time::interval(self.interval);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = ticker.tick() => {
                    for n in self.wallet.claim_pending_email_notifications(50).await? {
                        let channels=self.channels.list_enabled_email_by_tenant(domain::shared::ids::TenantId(n.tenant_id)).await?;
                        let subject=format!("PingVertex PAYG: {}", n.event_type.replace('_'," "));
                        let mut delivered=false;
                        for c in channels {
                            if self.notifications.notify_wallet_email(&c.target,&subject,&n.message).await.is_ok() {
                                delivered=true;
                            }
                        }
                        if delivered {
                            let _=self.wallet.mark_balance_notification_emailed(n.id).await?;
                        }
                    }
                }
            }
        }
    }
}
