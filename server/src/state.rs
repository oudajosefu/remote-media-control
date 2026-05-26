use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::config::{self, PersistedConfig};

#[derive(Debug, Clone)]
pub enum StateEvent {
    ActiveChanged(bool),
    PairingUrlRefreshed,
}

struct Inner {
    token: String,
    is_active: bool,
    auto_launch: bool,
    has_shown_pairing_qr: bool,
}

impl Inner {
    fn snapshot(&self) -> PersistedConfig {
        PersistedConfig {
            token: self.token.clone(),
            is_active: self.is_active,
            auto_launch: self.auto_launch,
            has_shown_pairing_qr: self.has_shown_pairing_qr,
        }
    }
}

pub struct AppState {
    inner: RwLock<Inner>,
    pub tx: broadcast::Sender<StateEvent>,
}

impl AppState {
    pub fn new(cfg: PersistedConfig) -> Arc<Self> {
        let (tx, _) = broadcast::channel(16);
        Arc::new(Self {
            inner: RwLock::new(Inner {
                token: cfg.token,
                is_active: cfg.is_active,
                auto_launch: cfg.auto_launch,
                has_shown_pairing_qr: cfg.has_shown_pairing_qr,
            }),
            tx,
        })
    }

    pub async fn token(&self) -> String {
        self.inner.read().await.token.clone()
    }

    pub async fn is_active(&self) -> bool {
        self.inner.read().await.is_active
    }

    pub async fn set_active(&self, next: bool) {
        let mut inner = self.inner.write().await;
        if inner.is_active == next {
            return;
        }
        inner.is_active = next;
        let cfg = inner.snapshot();
        drop(inner);
        config::save(&cfg);
        self.tx.send(StateEvent::ActiveChanged(next)).ok();
    }

    pub async fn set_auto_launch(&self, next: bool) {
        let mut inner = self.inner.write().await;
        inner.auto_launch = next;
        let cfg = inner.snapshot();
        drop(inner);
        config::save(&cfg);
    }

    pub fn mark_pairing_qr_shown(&self) {
        let mut inner = self.inner.blocking_write();
        if inner.has_shown_pairing_qr {
            return;
        }
        inner.has_shown_pairing_qr = true;
        let cfg = inner.snapshot();
        drop(inner);
        config::save(&cfg);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StateEvent> {
        self.tx.subscribe()
    }
}
