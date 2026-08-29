#![forbid(unsafe_code)]

//! Fixed-path, host-only configuration for CapyIO's MicYou process Adapter.
//!
//! The WebView has no access to this API. The executable path and Windows
//! endpoint identity are deliberately redacted from `Debug` output.

use std::{
    env,
    ffi::OsString,
    fmt, fs,
    fs::{File, OpenOptions},
    io::{Read, Write},
    net::IpAddr,
    path::{Path, PathBuf},
};

use capyio_micyou_adapter::{
    DEFAULT_MICYOU_PORT, MicYouConfig, MicYouInventory, MicYouProbe, MicYouSupervisor,
    PINNED_MICYOU_VERSION, ProbeLimits, SupervisorLimits,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CONFIG_SCHEMA_VERSION: u8 = 1;
pub const MAX_CONFIG_BYTES: u64 = 16 * 1024;
pub const ENV_EXE: &str = "CAPYIO_MICYOU_CLI";
pub const ENV_BIND_IP: &str = "CAPYIO_MICYOU_BIND_IP";
pub const ENV_PORT: &str = "CAPYIO_MICYOU_PORT";
pub const ENV_ENDPOINT_ID: &str = "CAPYIO_MICYOU_ENDPOINT_ID";
pub const ENV_ENDPOINT_NAME: &str = "CAPYIO_MICYOU_ENDPOINT_NAME";

const ENV_LOCAL_APP_DATA: &str = "LOCALAPPDATA";
const CONFIG_RELATIVE_COMPONENTS: [&str; 3] = ["CapyIO", "host", "micyou-v1.json"];
const OVERRIDE_NAMES: [&str; 5] = [
    ENV_EXE,
    ENV_BIND_IP,
    ENV_PORT,
    ENV_ENDPOINT_ID,
    ENV_ENDPOINT_NAME,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustedConfigSource {
    EnvironmentOverride,
    UserConfigFile,
}

#[derive(Clone)]
pub struct TrustedMicYouHostConfig {
    executable: PathBuf,
    bind_ip: IpAddr,
    port: u16,
    endpoint_id: String,
    endpoint_name: String,
}

impl fmt::Debug for TrustedMicYouHostConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrustedMicYouHostConfig")
            .field("executable", &"<redacted>")
            .field("bind_ip", &"<redacted>")
            .field("port", &self.port)
            .field("endpoint_id", &"<redacted>")
            .field("endpoint_name", &"<redacted>")
            .finish()
    }
}

impl TrustedMicYouHostConfig {
    pub fn new(
        executable: impl Into<PathBuf>,
        bind_ip: IpAddr,
        port: u16,
        endpoint_id: impl Into<String>,
        endpoint_name: impl Into<String>,
    ) -> Result<Self, HostConfigError> {
        let value = Self {
            executable: executable.into(),
            bind_ip,
            port,
            endpoint_id: endpoint_id.into(),
            endpoint_name: endpoint_name.into(),
        };
        value.adapter_config()?;
        Ok(value)
    }

    pub fn provision_from_inventory(
        executable: impl Into<PathBuf>,
        bind_ip: IpAddr,
        port: u16,
        endpoint_id: &str,
        inventory: &MicYouInventory,
    ) -> Result<Self, HostConfigError> {
        if inventory.version != PINNED_MICYOU_VERSION {
            return Err(HostConfigError::UnsupportedInventoryVersion);
        }
        let device = inventory
            .output_devices
            .iter()
            .find(|device| device.id == endpoint_id)
            .ok_or(HostConfigError::EndpointUnavailable)?;
        Self::new(
            executable,
            bind_ip,
            port,
            device.id.clone(),
            device.name.clone(),
        )
    }

    pub fn probe_and_provision(
        executable: impl Into<PathBuf>,
        bind_ip: IpAddr,
        port: u16,
        endpoint_id: &str,
    ) -> Result<Self, HostConfigError> {
        let executable = executable.into();
        let inventory = MicYouProbe::new(ProbeLimits::default())?
            .inventory(&executable)
            .map_err(HostConfigError::Adapter)?;
        Self::provision_from_inventory(executable, bind_ip, port, endpoint_id, &inventory)
    }

    pub fn adapter_config(&self) -> Result<MicYouConfig, HostConfigError> {
        MicYouConfig::new(
            self.executable.clone(),
            self.bind_ip,
            self.port,
            self.endpoint_id.clone(),
            self.endpoint_name.clone(),
        )
        .map_err(HostConfigError::Adapter)
    }

    pub fn supervisor(&self) -> Result<MicYouSupervisor, HostConfigError> {
        MicYouSupervisor::new(
            self.adapter_config()?,
            ProbeLimits::default(),
            SupervisorLimits::default(),
        )
        .map_err(HostConfigError::Adapter)
    }

    pub fn connection_hint(&self) -> String {
        format!("在 Android MicYou 中连接 {}:{}", self.bind_ip, self.port)
    }
}

#[derive(Debug)]
pub struct LoadedTrustedMicYouHostConfig {
    pub config: TrustedMicYouHostConfig,
    pub source: TrustedConfigSource,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiskConfigV1 {
    schema_version: u8,
    executable: String,
    bind_ip: IpAddr,
    port: u16,
    output_device_id: String,
    output_device_name: String,
}

impl TryFrom<DiskConfigV1> for TrustedMicYouHostConfig {
    type Error = HostConfigError;

    fn try_from(value: DiskConfigV1) -> Result<Self, Self::Error> {
        if value.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(HostConfigError::UnsupportedSchemaVersion);
        }
        Self::new(
            value.executable,
            value.bind_ip,
            value.port,
            value.output_device_id,
            value.output_device_name,
        )
    }
}

impl TryFrom<&TrustedMicYouHostConfig> for DiskConfigV1 {
    type Error = HostConfigError;

    fn try_from(value: &TrustedMicYouHostConfig) -> Result<Self, Self::Error> {
        let executable = value
            .executable
            .to_str()
            .ok_or(HostConfigError::NonUnicodeExecutablePath)?
            .to_owned();
        Ok(Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            executable,
            bind_ip: value.bind_ip,
            port: value.port,
            output_device_id: value.endpoint_id.clone(),
            output_device_name: value.endpoint_name.clone(),
        })
    }
}

pub fn load_trusted_host_config() -> Result<LoadedTrustedMicYouHostConfig, HostConfigError> {
    if OVERRIDE_NAMES
        .iter()
        .any(|name| env::var_os(name).is_some())
    {
        return Ok(LoadedTrustedMicYouHostConfig {
            config: from_environment_with(|name| env::var_os(name))?,
            source: TrustedConfigSource::EnvironmentOverride,
        });
    }
    let path = default_config_path()?;
    Ok(LoadedTrustedMicYouHostConfig {
        config: load_from_path(&path)?,
        source: TrustedConfigSource::UserConfigFile,
    })
}

pub fn default_config_path() -> Result<PathBuf, HostConfigError> {
    let root = env::var_os(ENV_LOCAL_APP_DATA).ok_or(HostConfigError::LocalAppDataUnavailable)?;
    default_config_path_from_local_app_data(Path::new(&root))
}

pub fn default_config_path_from_local_app_data(
    local_app_data: &Path,
) -> Result<PathBuf, HostConfigError> {
    if local_app_data.as_os_str().is_empty() {
        return Err(HostConfigError::LocalAppDataUnavailable);
    }
    let mut path = local_app_data.to_path_buf();
    for component in CONFIG_RELATIVE_COMPONENTS {
        path.push(component);
    }
    Ok(path)
}

pub fn load_from_path(path: &Path) -> Result<TrustedMicYouHostConfig, HostConfigError> {
    let file = File::open(path).map_err(|source| HostConfigError::Read { source })?;
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| HostConfigError::Read { source })?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(HostConfigError::ConfigTooLarge);
    }
    serde_json::from_slice::<DiskConfigV1>(&bytes)
        .map_err(HostConfigError::MalformedJson)?
        .try_into()
}

pub fn write_new_default_config(config: &TrustedMicYouHostConfig) -> Result<(), HostConfigError> {
    write_new_config(&default_config_path()?, config)
}

pub fn write_new_config(
    path: &Path,
    config: &TrustedMicYouHostConfig,
) -> Result<(), HostConfigError> {
    let parent = path.parent().ok_or(HostConfigError::InvalidConfigPath)?;
    fs::create_dir_all(parent).map_err(|source| HostConfigError::CreateDirectory { source })?;
    let disk = DiskConfigV1::try_from(config)?;
    let mut bytes = serde_json::to_vec_pretty(&disk).map_err(HostConfigError::Serialize)?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(HostConfigError::ConfigTooLarge);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists {
                HostConfigError::ConfigAlreadyExists
            } else {
                HostConfigError::Write { source }
            }
        })?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|source| HostConfigError::Write { source })
}

fn from_environment_with(
    mut get: impl FnMut(&str) -> Option<OsString>,
) -> Result<TrustedMicYouHostConfig, HostConfigError> {
    let executable = required_os_value(&mut get, ENV_EXE)?;
    let bind_ip = required_unicode_value(&mut get, ENV_BIND_IP)?
        .parse::<IpAddr>()
        .map_err(|_| HostConfigError::InvalidEnvironmentValue(ENV_BIND_IP))?;
    let port = match get(ENV_PORT) {
        Some(value) => value
            .into_string()
            .map_err(|_| HostConfigError::InvalidEnvironmentValue(ENV_PORT))?
            .parse::<u16>()
            .map_err(|_| HostConfigError::InvalidEnvironmentValue(ENV_PORT))?,
        None => DEFAULT_MICYOU_PORT,
    };
    TrustedMicYouHostConfig::new(
        PathBuf::from(executable),
        bind_ip,
        port,
        required_unicode_value(&mut get, ENV_ENDPOINT_ID)?,
        required_unicode_value(&mut get, ENV_ENDPOINT_NAME)?,
    )
}

fn required_os_value(
    get: &mut impl FnMut(&str) -> Option<OsString>,
    name: &'static str,
) -> Result<OsString, HostConfigError> {
    get(name).ok_or(HostConfigError::IncompleteEnvironmentOverride(name))
}

fn required_unicode_value(
    get: &mut impl FnMut(&str) -> Option<OsString>,
    name: &'static str,
) -> Result<String, HostConfigError> {
    required_os_value(get, name)?
        .into_string()
        .map_err(|_| HostConfigError::InvalidEnvironmentValue(name))
}

#[derive(Debug, Error)]
pub enum HostConfigError {
    #[error("the trusted MicYou host environment override is incomplete: missing {0}")]
    IncompleteEnvironmentOverride(&'static str),
    #[error("the trusted MicYou host environment value is invalid: {0}")]
    InvalidEnvironmentValue(&'static str),
    #[error("the user-local application data directory is unavailable")]
    LocalAppDataUnavailable,
    #[error("the trusted MicYou configuration path is invalid")]
    InvalidConfigPath,
    #[error("the trusted MicYou configuration could not be read: {source}")]
    Read { source: std::io::Error },
    #[error("the trusted MicYou configuration is larger than the fixed limit")]
    ConfigTooLarge,
    #[error("the trusted MicYou configuration JSON is invalid: {0}")]
    MalformedJson(serde_json::Error),
    #[error("the trusted MicYou configuration schema version is unsupported")]
    UnsupportedSchemaVersion,
    #[error("the MicYou inventory version is unsupported")]
    UnsupportedInventoryVersion,
    #[error("the selected microphone ingress endpoint is unavailable")]
    EndpointUnavailable,
    #[error("the trusted MicYou executable path is not Unicode")]
    NonUnicodeExecutablePath,
    #[error("the trusted MicYou configuration could not be serialized: {0}")]
    Serialize(serde_json::Error),
    #[error("the trusted MicYou configuration directory could not be created: {source}")]
    CreateDirectory { source: std::io::Error },
    #[error("trusted MicYou configuration already exists; it was not overwritten")]
    ConfigAlreadyExists,
    #[error("the trusted MicYou configuration could not be written: {source}")]
    Write { source: std::io::Error },
    #[error("MicYou host validation failed: {0}")]
    Adapter(#[from] capyio_micyou_adapter::MicYouError),
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        num::NonZeroUsize,
        time::{SystemTime, UNIX_EPOCH},
    };

    use capyio_micyou_adapter::MicYouOutputDevice;

    use super::*;

    fn fixture_config() -> TrustedMicYouHostConfig {
        TrustedMicYouHostConfig::new(
            "private-micyou.exe",
            "100.64.0.10".parse().expect("IP"),
            8554,
            "{0.0.0.00000000}.{fixture-ingress}",
            "CapyIO Microphone Ingress",
        )
        .expect("fixture config")
    }

    fn unique_temp_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        env::temp_dir()
            .join(format!(
                "capyio-micyou-config-{}-{nonce}",
                std::process::id()
            ))
            .join("micyou-v1.json")
    }

    #[test]
    fn fixed_default_path_never_uses_webview_input() {
        let path = default_config_path_from_local_app_data(Path::new("C:/fixture/local"))
            .expect("default path");
        assert_eq!(
            path,
            PathBuf::from("C:/fixture/local/CapyIO/host/micyou-v1.json")
        );
    }

    #[test]
    fn complete_environment_override_parses_and_partial_override_fails_closed() {
        let mut values = BTreeMap::from([
            (ENV_EXE, OsString::from("private-micyou.exe")),
            (ENV_BIND_IP, OsString::from("100.64.0.10")),
            (
                ENV_ENDPOINT_ID,
                OsString::from("{0.0.0.00000000}.{fixture-ingress}"),
            ),
            (
                ENV_ENDPOINT_NAME,
                OsString::from("CapyIO Microphone Ingress"),
            ),
        ]);
        let config =
            from_environment_with(|name| values.get(name).cloned()).expect("complete override");
        assert_eq!(
            config.connection_hint(),
            "在 Android MicYou 中连接 100.64.0.10:8554"
        );
        values.remove(ENV_ENDPOINT_ID);
        assert!(matches!(
            from_environment_with(|name| values.get(name).cloned()),
            Err(HostConfigError::IncompleteEnvironmentOverride(
                ENV_ENDPOINT_ID
            ))
        ));
    }

    #[test]
    fn provisioning_selects_exact_id_and_derives_name() {
        let inventory = MicYouInventory {
            version: PINNED_MICYOU_VERSION.to_owned(),
            output_devices: vec![
                MicYouOutputDevice {
                    index: NonZeroUsize::new(1).expect("index"),
                    id: "speaker-id".to_owned(),
                    name: "Duplicated localized name".to_owned(),
                },
                MicYouOutputDevice {
                    index: NonZeroUsize::new(2).expect("index"),
                    id: "ingress-id".to_owned(),
                    name: "Duplicated localized name".to_owned(),
                },
            ],
        };
        let config = TrustedMicYouHostConfig::provision_from_inventory(
            "private-micyou.exe",
            "100.64.0.10".parse().expect("IP"),
            8554,
            "ingress-id",
            &inventory,
        )
        .expect("stable ID selection");
        let adapter = config.adapter_config().expect("adapter config");
        assert_eq!(adapter.output_device_id(), "ingress-id");
        assert_eq!(adapter.output_device(), "Duplicated localized name");
    }

    #[test]
    fn disk_round_trip_denies_overwrite_and_debug_redacts_private_values() {
        let path = unique_temp_path();
        let config = fixture_config();
        write_new_config(&path, &config).expect("write new config");
        let loaded = load_from_path(&path).expect("load config");
        assert_eq!(loaded.connection_hint(), config.connection_hint());
        assert!(matches!(
            write_new_config(&path, &config),
            Err(HostConfigError::ConfigAlreadyExists)
        ));
        let debug = format!("{loaded:?}");
        assert!(!debug.contains("private-micyou"));
        assert!(!debug.contains("fixture-ingress"));
        fs::remove_dir_all(path.parent().expect("parent")).expect("remove fixture directory");
    }

    #[test]
    fn unknown_fields_and_schema_versions_fail_closed() {
        let path = unique_temp_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("fixture directory");
        fs::write(
            &path,
            r#"{"schemaVersion":1,"executable":"micyou.exe","bindIp":"100.64.0.10","port":8554,"outputDeviceId":"id","outputDeviceName":"name","webviewPath":"untrusted"}"#,
        )
        .expect("unknown-field fixture");
        assert!(matches!(
            load_from_path(&path),
            Err(HostConfigError::MalformedJson(_))
        ));
        fs::write(
            &path,
            r#"{"schemaVersion":2,"executable":"micyou.exe","bindIp":"100.64.0.10","port":8554,"outputDeviceId":"id","outputDeviceName":"name"}"#,
        )
        .expect("version fixture");
        assert!(matches!(
            load_from_path(&path),
            Err(HostConfigError::UnsupportedSchemaVersion)
        ));
        fs::remove_dir_all(path.parent().expect("parent")).expect("remove fixture directory");
    }
}
