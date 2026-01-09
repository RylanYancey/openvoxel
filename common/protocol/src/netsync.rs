use bevy::reflect::TypePath;

/// Metadata about a component update sent from the Server to the Client.
#[derive(Copy, Clone, TypePath)]
#[type_path = "protocol"]
pub struct SyncUpdate<T> {
    /// The Version of the entity to
    /// resolve ordering issues.
    pub version: u16,

    /// Sync ID of the target Entity.
    pub sync_id: u16,

    /// Component data.
    pub payload: T,
}
