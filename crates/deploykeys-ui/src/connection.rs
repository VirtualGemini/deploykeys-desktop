//! Global connection state.
//!
//! The app operates "through" exactly one active connection at a time. Keys and
//! repository actions run against whichever connection is currently connected.
//! For now the only connection is the local machine (this device); server
//! connections arrive in a later stage.

use leptos::*;
use wasm_bindgen_futures::spawn_local;

/// Stable id of the built-in local connection.
pub const LOCAL_ID: &str = "local";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionKind {
    Local,
}

impl ConnectionKind {
    /// i18n key for the connection's display name.
    pub fn name_key(self) -> &'static str {
        match self {
            ConnectionKind::Local => "connect.local_name",
        }
    }

    /// i18n key for the connection's type label.
    pub fn type_key(self) -> &'static str {
        match self {
            ConnectionKind::Local => "connect.type_local",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub id: String,
    pub kind: ConnectionKind,
}

impl Connection {
    pub fn local() -> Self {
        Self {
            id: LOCAL_ID.to_string(),
            kind: ConnectionKind::Local,
        }
    }
}

/// Reactive connection registry plus which one is currently connected. Provided
/// at the app root so every screen shares one source of truth. At most one
/// connection is connected at a time; `connected_id == None` means all offline.
#[derive(Clone, Copy)]
pub struct ConnectionState {
    pub connections: RwSignal<Vec<Connection>>,
    pub connected_id: RwSignal<Option<String>>,
}

impl ConnectionState {
    /// Make `id` the single connected connection (disconnects any other).
    pub fn connect(self, id: &str) {
        self.connected_id.set(Some(id.to_string()));
        self.persist();
    }

    /// Take `id` offline if it is the connected one; no-op otherwise.
    pub fn disconnect(self, id: &str) {
        if self.connected_id.get_untracked().as_deref() == Some(id) {
            self.connected_id.set(None);
            self.persist();
        }
    }

    /// Write the current connected id to the backend so the choice survives
    /// navigation and app restarts. An empty string records "all offline".
    fn persist(self) {
        let value = self.connected_id.get_untracked().unwrap_or_default();
        spawn_local(async move {
            if let Err(e) = crate::api::set_active_connection(&value).await {
                leptos::logging::warn!("Failed to persist active connection: {e}");
            }
        });
    }

    /// Apply a persisted value loaded from the backend: empty string = offline.
    pub fn apply_persisted(self, value: String) {
        self.connected_id
            .set(if value.is_empty() { None } else { Some(value) });
    }

    /// Reactive read: is `id` the connected connection?
    pub fn is_connected(self, id: &str) -> bool {
        self.connected_id.get().as_deref() == Some(id)
    }

    /// Reactive read: is any connection currently connected? When false, the
    /// app has no environment to operate through, so connection-bound actions
    /// (managing keys, cloning, binding deploy keys) must be blocked.
    pub fn has_active(self) -> bool {
        self.connected_id.get().is_some()
    }
}

/// Provide the connection state at the app root. Seeds a single local
/// connection, connected by default — the app starts operating on this device.
pub fn provide_connection_state() {
    provide_context(ConnectionState {
        connections: RwSignal::new(vec![Connection::local()]),
        connected_id: RwSignal::new(Some(LOCAL_ID.to_string())),
    });
}

pub fn connection_state() -> ConnectionState {
    use_context::<ConnectionState>().expect("connection state provided at root")
}
