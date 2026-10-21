//! Optional Secret Service client over an explicitly selected local Unix bus.
//! The standard `plain` session does NOT encrypt local D-Bus traffic. At-rest
//! protection and successful-write durability are supplied by the OS keyring.
//! No plaintext file fallback, network bus, activation, prompt or unlock exists.

use super::{BackendError, CredentialBackend, SecretBytes, handle_for};
use capyio_control_credentials::{ControlCredentialBundle, MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES};
use capyio_core::NodeId;
use capyio_pairing::CredentialHandle;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Mutex, time::Duration};
use zbus::{
    Connection, Proxy,
    proxy::{CacheProperties, MethodFlags},
    zvariant::{OwnedObjectPath, OwnedValue, Type, Value},
};

const SERVICE: &str = "org.freedesktop.secrets";
const ROOT: &str = "/org/freedesktop/secrets";
const SERVICE_IFACE: &str = "org.freedesktop.Secret.Service";
const COLLECTION_IFACE: &str = "org.freedesktop.Secret.Collection";
const ITEM_IFACE: &str = "org.freedesktop.Secret.Item";
const CONTENT_TYPE: &str = "application/x-capyio-control-credential";
const MAX_PATH: usize = 1024;

/// A synchronous, optional OS keyring backend. Construction only checks the
/// explicitly selected existing unlocked collection; it does not read items.
/// Calls must be made outside an async runtime and are bounded by one overall
/// deadline including connect/authentication. Each call owns and closes its bus
/// connection; no background thread, daemon installation or environment lookup.
///
/// The application's accepted secret is capped at 64 KiB. zbus 5.19 separately
/// caps complete incoming messages at 128 MiB before allocation; this is NOT a
/// 64 KiB transport-memory limit. The trusted bus/keyring can allocate up to that
/// library limit. Queue capacity is one; decoded results have tighter limits.
pub struct SecretServiceBackend {
    socket: String,
    expected_guid: Option<String>,
    collection: String,
    owner: NodeId,
    timeout: Duration,
    owner_pid: u32,
    operation: Mutex<()>,
}

enum Operation<'a> {
    Preflight,
    Load(&'a CredentialHandle),
    Put(&'a CredentialHandle, &'a [u8]),
}

impl SecretServiceBackend {
    /// Only `unix:path=/absolute/path[,guid=<32 hex>]` is supported. Abstract,
    /// TCP, autolaunch, multiple and escaped bus addresses fail closed. The
    /// connected Unix peer must have the caller's effective UID.
    /// The caller selects a persistent collection, never the session collection.
    pub fn connect_unix(
        address: &str,
        collection: &str,
        owner: NodeId,
        timeout: Duration,
    ) -> Result<Self, BackendError> {
        let (socket, expected_guid) = parse_address(address)?;
        if owner.as_uuid().is_nil()
            || timeout.is_zero()
            || timeout > Duration::from_secs(10)
            || collection.len() > MAX_PATH
            || !collection.starts_with("/org/freedesktop/secrets/collection/")
            || collection == "/org/freedesktop/secrets/collection/session"
            || OwnedObjectPath::try_from(collection).is_err()
        {
            return Err(BackendError::Invalid);
        }
        let backend = Self {
            socket,
            expected_guid,
            collection: collection.to_owned(),
            owner,
            timeout,
            owner_pid: std::process::id(),
            operation: Mutex::new(()),
        };
        backend.run(Operation::Preflight)?;
        Ok(backend)
    }
    fn run(&self, operation: Operation<'_>) -> Result<Option<SecretBytes>, BackendError> {
        if self.owner_pid != std::process::id() || tokio::runtime::Handle::try_current().is_ok() {
            return Err(BackendError::Unavailable);
        }
        let _guard = self
            .operation
            .try_lock()
            .map_err(|_| BackendError::Conflict)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .map_err(|_| BackendError::Unavailable)?;
        runtime.block_on(async {
            tokio::time::timeout(self.timeout, async {
                let stream = tokio::net::UnixStream::connect(&self.socket)
                    .await
                    .map_err(|_| BackendError::Unavailable)?;
                if stream
                    .peer_cred()
                    .map_err(|_| BackendError::Unavailable)?
                    .uid()
                    != rustix::process::geteuid().as_raw()
                {
                    return Err(BackendError::Unavailable);
                }
                let connection = zbus::connection::Builder::unix_stream(stream)
                    .max_queued(1)
                    .method_timeout(self.timeout)
                    .build()
                    .await
                    .map_err(map_error)?;
                if let Some(guid) = &self.expected_guid
                    && connection.server_guid().as_str() != guid
                {
                    return Err(BackendError::Unavailable);
                }
                let result = async {
                    let bus = zbus::fdo::DBusProxy::new(&connection)
                        .await
                        .map_err(map_error)?;
                    // GetNameOwner never activates a missing service. All subsequent
                    // service calls target this unique owner, never the well-known name.
                    let owner = bus
                        .get_name_owner(SERVICE.try_into().map_err(|_| BackendError::Invalid)?)
                        .await
                        .map_err(|_| BackendError::Unavailable)?;
                    let namespace =
                        format!("org.capyio.Pairing.n{}", self.owner.as_uuid().simple());
                    let reply = connection
                        .request_name_with_flags(
                            namespace.as_str(),
                            zbus::fdo::RequestNameFlags::DoNotQueue.into(),
                        )
                        .await
                        .map_err(map_namespace_error)?;
                    if reply != zbus::fdo::RequestNameReply::PrimaryOwner {
                        return Err(BackendError::Conflict);
                    }
                    let mut context = Context {
                        connection: &connection,
                        destination: owner.to_string(),
                        collection: &self.collection,
                        owner: self.owner,
                        session: None,
                    };
                    context
                        .check_unlocked(&self.collection, COLLECTION_IFACE)
                        .await?;
                    if matches!(operation, Operation::Preflight) {
                        return Ok(None);
                    }
                    let (output, session): (OwnedValue, OwnedObjectPath) = context
                        .call(
                            ROOT,
                            SERVICE_IFACE,
                            "OpenSession",
                            &("plain", Value::from("")),
                        )
                        .await?;
                    let output: String = output.try_into().map_err(|_| BackendError::Invalid)?;
                    if !output.is_empty()
                        || session.as_str() == "/"
                        || session.as_str().len() > MAX_PATH
                        || !session
                            .as_str()
                            .starts_with("/org/freedesktop/secrets/session/")
                    {
                        return Err(BackendError::Invalid);
                    }
                    context.session = Some(session);
                    match operation {
                        Operation::Preflight => Ok(None),
                        Operation::Load(handle) => context.load(handle).await.map(Some),
                        Operation::Put(handle, bytes) => {
                            context.put(handle, bytes).await?;
                            Ok(None)
                        }
                    }
                }
                .await;
                // Closing disconnects our ephemeral protocol session and releases
                // cooperative namespace ownership. No item/collection deletion.
                let _ = connection.close().await;
                result
            })
            .await
            .map_err(|_| BackendError::Unavailable)?
        })
    }
}
impl CredentialBackend for SecretServiceBackend {
    fn load(&self, owner: NodeId, handle: &CredentialHandle) -> Result<SecretBytes, BackendError> {
        if owner != self.owner {
            return Err(BackendError::Invalid);
        }
        validate_handle(handle)?;
        self.run(Operation::Load(handle))?
            .ok_or(BackendError::Missing)
    }
    fn put_exact(
        &self,
        owner: NodeId,
        handle: &CredentialHandle,
        bytes: &[u8],
    ) -> Result<(), BackendError> {
        if owner != self.owner {
            return Err(BackendError::Invalid);
        }
        validate_bundle(handle, bytes)?;
        self.run(Operation::Put(handle, bytes))?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Type)]
struct WireSecret {
    session: OwnedObjectPath,
    parameters: Vec<u8>,
    value: Vec<u8>,
    content_type: String,
}
impl Drop for WireSecret {
    fn drop(&mut self) {
        self.value.fill(0);
        self.parameters.fill(0);
    }
}
struct Context<'a> {
    connection: &'a Connection,
    destination: String,
    collection: &'a str,
    owner: NodeId,
    session: Option<OwnedObjectPath>,
}
impl Context<'_> {
    async fn call<B, R>(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        body: &B,
    ) -> Result<R, BackendError>
    where
        B: Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        let proxy = zbus::proxy::Builder::<Proxy<'_>>::new(self.connection)
            .destination(self.destination.as_str())
            .map_err(map_error)?
            .path(path)
            .map_err(map_error)?
            .interface(interface)
            .map_err(map_error)?
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .map_err(map_error)?;
        // No cached properties or interactive authorization; explicit NoAutoStart
        // is redundant for a unique destination, but enforces the intended policy.
        proxy
            .call_with_flags(method, MethodFlags::NoAutoStart.into(), body)
            .await
            .map_err(map_error)?
            .ok_or(BackendError::Invalid)
    }
    async fn property(
        &self,
        path: &str,
        interface: &str,
        name: &str,
    ) -> Result<OwnedValue, BackendError> {
        self.call(
            path,
            "org.freedesktop.DBus.Properties",
            "Get",
            &(interface, name),
        )
        .await
    }
    async fn check_unlocked(&self, path: &str, interface: &str) -> Result<(), BackendError> {
        let locked: bool = self
            .property(path, interface, "Locked")
            .await?
            .try_into()
            .map_err(|_| BackendError::Invalid)?;
        if locked {
            Err(BackendError::Locked)
        } else {
            Ok(())
        }
    }
    fn attributes(&self, handle: &CredentialHandle) -> HashMap<String, String> {
        HashMap::from([
            ("application".into(), "dev.capyio.control".into()),
            ("schema".into(), "1".into()),
            ("local-node-id".into(), self.owner.to_string()),
            ("credential-handle".into(), handle.as_str().into()),
        ])
    }
    fn item_path(&self, path: &str) -> Result<(), BackendError> {
        if path.len() > MAX_PATH
            || !path
                .strip_prefix(self.collection)
                .is_some_and(|tail| tail.starts_with('/') && tail.len() > 1)
        {
            return Err(BackendError::Invalid);
        }
        Ok(())
    }
    async fn search(
        &self,
        handle: &CredentialHandle,
    ) -> Result<Option<OwnedObjectPath>, BackendError> {
        self.check_unlocked(self.collection, COLLECTION_IFACE)
            .await?;
        let mut paths: Vec<OwnedObjectPath> = self
            .call(
                self.collection,
                COLLECTION_IFACE,
                "SearchItems",
                &(self.attributes(handle),),
            )
            .await?;
        if paths.len() > 1 {
            return Err(BackendError::Conflict);
        }
        let Some(path) = paths.pop() else {
            return Ok(None);
        };
        self.item_path(path.as_str())?;
        self.check_unlocked(path.as_str(), ITEM_IFACE).await?;
        let attrs: HashMap<String, String> = self
            .property(path.as_str(), ITEM_IFACE, "Attributes")
            .await?
            .try_into()
            .map_err(|_| BackendError::Invalid)?;
        if attrs != self.attributes(handle) {
            return Err(BackendError::Conflict);
        }
        Ok(Some(path))
    }
    async fn read_item(
        &self,
        path: &OwnedObjectPath,
        handle: &CredentialHandle,
    ) -> Result<SecretBytes, BackendError> {
        let session = self.session.as_ref().ok_or(BackendError::Invalid)?;
        let (mut secret,): (WireSecret,) = self
            .call(path.as_str(), ITEM_IFACE, "GetSecret", &(session,))
            .await?;
        if secret.session != *session
            || !secret.parameters.is_empty()
            || secret.content_type != CONTENT_TYPE
        {
            return Err(BackendError::Invalid);
        }
        validate_bundle(handle, &secret.value)?;
        SecretBytes::new(std::mem::take(&mut secret.value))
    }
    async fn load(&self, handle: &CredentialHandle) -> Result<SecretBytes, BackendError> {
        let path = self.search(handle).await?.ok_or(BackendError::Missing)?;
        self.read_item(&path, handle).await
    }
    async fn put(&self, handle: &CredentialHandle, bytes: &[u8]) -> Result<(), BackendError> {
        if let Some(path) = self.search(handle).await? {
            let existing = self.read_item(&path, handle).await?;
            return if existing.as_slice() == bytes {
                Ok(())
            } else {
                Err(BackendError::Conflict)
            };
        }
        let properties = HashMap::from([
            (
                "org.freedesktop.Secret.Item.Label",
                Value::from("CapyIO control credential"),
            ),
            (
                "org.freedesktop.Secret.Item.Attributes",
                Value::from(self.attributes(handle)),
            ),
        ]);
        let secret = WireSecret {
            session: self.session.as_ref().ok_or(BackendError::Invalid)?.clone(),
            parameters: vec![],
            value: bytes.to_vec(),
            content_type: CONTENT_TYPE.into(),
        };
        let (item, prompt): (OwnedObjectPath, OwnedObjectPath) = self
            .call(
                self.collection,
                COLLECTION_IFACE,
                "CreateItem",
                &(properties, secret, false),
            )
            .await?;
        if prompt.as_str() != "/" || item.as_str() == "/" {
            return Err(BackendError::Unavailable);
        }
        self.item_path(item.as_str())?;
        // A standard service has no attribute CAS. Cooperative clients serialize
        // using the bus name; other duplicate results fail closed, without delete.
        let selected = self.search(handle).await?.ok_or(BackendError::Missing)?;
        if selected != item {
            return Err(BackendError::Conflict);
        }
        let stored = self.read_item(&selected, handle).await?;
        if stored.as_slice() != bytes {
            return Err(BackendError::Conflict);
        }
        Ok(())
    }
}
fn validate_handle(handle: &CredentialHandle) -> Result<(), BackendError> {
    let suffix = handle
        .as_str()
        .strip_prefix(super::HANDLE_PREFIX)
        .ok_or(BackendError::Invalid)?;
    if suffix.len() != 64
        || !suffix
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(BackendError::Invalid);
    }
    Ok(())
}
fn validate_bundle(handle: &CredentialHandle, bytes: &[u8]) -> Result<(), BackendError> {
    validate_handle(handle)?;
    if bytes.is_empty() || bytes.len() > MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES {
        return Err(BackendError::Invalid);
    }
    if handle_for(Sha256::digest(bytes).into()) != handle.as_str() {
        return Err(BackendError::Conflict);
    }
    ControlCredentialBundle::decode(bytes).map_err(|_| BackendError::Invalid)?;
    Ok(())
}
fn parse_address(address: &str) -> Result<(String, Option<String>), BackendError> {
    if address.len() > 4096 {
        return Err(BackendError::Invalid);
    }
    let mut parts = address.split(',');
    let path = parts
        .next()
        .and_then(|s| s.strip_prefix("unix:path="))
        .ok_or(BackendError::Invalid)?;
    if !path.starts_with('/')
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
    {
        return Err(BackendError::Invalid);
    }
    let guid = parts
        .next()
        .map(|s| {
            s.strip_prefix("guid=")
                .filter(|g| g.len() == 32 && g.bytes().all(|b| b.is_ascii_hexdigit()))
                .map(str::to_owned)
                .ok_or(BackendError::Invalid)
        })
        .transpose()?;
    if parts.next().is_some() {
        return Err(BackendError::Invalid);
    }
    Ok((path.into(), guid))
}
fn map_error(error: zbus::Error) -> BackendError {
    match error {
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.Secret.Error.IsLocked" =>
        {
            BackendError::Locked
        }
        zbus::Error::MethodError(name, _, _)
            if matches!(
                name.as_str(),
                "org.freedesktop.Secret.Error.NoSuchObject"
                    | "org.freedesktop.DBus.Error.UnknownObject"
            ) =>
        {
            BackendError::Missing
        }
        _ => BackendError::Unavailable,
    }
}

// zbus turns RequestNameReply::Exists into NameTaken before returning the reply.
// Preserve the contention category, without treating transport errors as conflicts.
fn map_namespace_error(error: zbus::Error) -> BackendError {
    match error {
        zbus::Error::NameTaken => BackendError::Conflict,
        other => map_error(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_name_taken_is_conflict_not_transport_unavailability() {
        assert_eq!(
            map_namespace_error(zbus::Error::NameTaken),
            BackendError::Conflict
        );
        assert_eq!(
            map_namespace_error(zbus::Error::Failure("synthetic transport failure".into())),
            BackendError::Unavailable
        );
    }
}
