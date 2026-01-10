//! Outer.sh server - collaborative AI conversation interface

pub mod crdt;
pub mod delegation;
pub mod error;
pub mod models;
pub mod opencode;
pub mod store;
pub mod websocket;

use sqlx::SqlitePool;
use std::sync::Arc;

/// Application state shared across handlers
pub struct AppState {
    pub store: store::Store,
    pub room_manager: crdt::room::RoomManager,
    pub delegation_manager: delegation::DelegationManager,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Arc<Self> {
        Arc::new(Self {
            store: store::Store::new(pool),
            room_manager: crdt::room::RoomManager::new(),
            delegation_manager: delegation::DelegationManager::new(),
        })
    }
}
