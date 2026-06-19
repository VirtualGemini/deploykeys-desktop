//! Global connection state.
//!
//! The app operates "through" exactly one active connection at a time. Keys and
//! repository actions run against whichever connection is currently connected.
//! For now the only connection is the local machine (this device); server
//! connections arrive in a later stage.

use leptos::*;

/// Stable id of the built-in local connection.
pub const LOCAL_ID: &str = "local";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionKind {
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub id: String,
    pub alias: String,
    pub kind: ConnectionKind,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub auth_method: Option<String>,
    pub key_base_dir: String,
}

impl Connection {
    pub fn local() -> Self {
        Self {
            id: LOCAL_ID.to_string(),
            alias: "Local".to_string(),
            kind: ConnectionKind::Local,
            host: None,
            port: None,
            username: None,
            auth_method: None,
            key_base_dir: String::new(),
        }
    }

    pub fn from_dto(dto: crate::api::ConnectionDto) -> Self {
        let kind = if dto.kind == "remote" {
            ConnectionKind::Remote
        } else {
            ConnectionKind::Local
        };
        Self {
            id: dto.id,
            alias: dto.alias,
            kind,
            host: dto.host,
            port: dto.port,
            username: dto.username,
            auth_method: dto.auth_method,
            key_base_dir: dto.key_base_dir,
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
    }

    /// Take `id` offline if it is the connected one; no-op otherwise.
    pub fn disconnect(self, id: &str) {
        if self.connected_id.get_untracked().as_deref() == Some(id) {
            self.connected_id.set(None);
        }
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

    pub fn set_connections(self, next: Vec<Connection>) {
        self.connections.set(next);
        let active = self.connected_id.get_untracked();
        if let Some(active) = active {
            if !self
                .connections
                .get_untracked()
                .iter()
                .any(|connection| connection.id == active)
            {
                self.connected_id.set(None);
            }
        }
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
