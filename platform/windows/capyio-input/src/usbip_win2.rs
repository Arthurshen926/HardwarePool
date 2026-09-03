use std::{
    ffi::{OsStr, OsString},
    fmt::{self, Display, Formatter},
    io::{self, Read},
    net::SocketAddrV4,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

pub const PINNED_USBIP_WIN2_VERSION: &str = "0.9.7.7";
pub const USBIP_XBOX360_VENDOR_ID: u16 = 0x045e;
pub const USBIP_XBOX360_PRODUCT_ID: u16 = 0x028e;
pub const USBIP_DS4_VENDOR_ID: u16 = 0x054c;
pub const USBIP_DS4_PRODUCT_ID: u16 = 0x09cc;
pub const MAX_USBIP_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_USBIP_OUTPUT_BYTES: usize = 64 * 1024;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_BUS_ID_BYTES: usize = 21;
const MAX_DEVICE_LABEL_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbipControllerKind {
    Xbox360,
    DualShock4,
}

impl UsbipControllerKind {
    const fn vendor_id(self) -> u16 {
        match self {
            Self::Xbox360 => USBIP_XBOX360_VENDOR_ID,
            Self::DualShock4 => USBIP_DS4_VENDOR_ID,
        }
    }

    const fn product_id(self) -> u16 {
        match self {
            Self::Xbox360 => USBIP_XBOX360_PRODUCT_ID,
            Self::DualShock4 => USBIP_DS4_PRODUCT_ID,
        }
    }
}

/// Explicit host assertion required before the mutating attachment command.
///
/// The caller must independently verify the pinned package digest/signature,
/// driver health, completion of any installer-required restart and operator
/// authorization for the first attachment. This token records that assertion;
/// it cannot prove external Windows state. Read-only probe/list do not need it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbipWin2DeploymentVerified(());

impl UsbipWin2DeploymentVerified {
    #[must_use]
    pub const fn confirmed_by_caller() -> Self {
        Self(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsbipWin2Config {
    executable: PathBuf,
    server: SocketAddrV4,
    command_timeout: Duration,
    output_limit: usize,
}

impl UsbipWin2Config {
    pub fn new(
        executable: PathBuf,
        server: SocketAddrV4,
        command_timeout: Duration,
        output_limit: usize,
    ) -> Result<Self, UsbipWin2Error> {
        if !executable.is_absolute() {
            return Err(UsbipWin2Error::ExecutablePathNotAbsolute(executable));
        }
        if !executable
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.eq_ignore_ascii_case("usbip.exe"))
        {
            return Err(UsbipWin2Error::UnexpectedExecutableName(executable));
        }
        if !server.ip().is_loopback() {
            return Err(UsbipWin2Error::NonLoopbackServer(server));
        }
        if server.port() == 0 {
            return Err(UsbipWin2Error::InvalidServerPort);
        }
        if command_timeout.is_zero() || command_timeout > MAX_USBIP_COMMAND_TIMEOUT {
            return Err(UsbipWin2Error::InvalidCommandTimeout(command_timeout));
        }
        if !(1..=MAX_USBIP_OUTPUT_BYTES).contains(&output_limit) {
            return Err(UsbipWin2Error::InvalidOutputLimit {
                actual: output_limit,
                maximum: MAX_USBIP_OUTPUT_BYTES,
            });
        }
        Ok(Self {
            executable,
            server,
            command_timeout,
            output_limit,
        })
    }

    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    #[must_use]
    pub const fn server(&self) -> SocketAddrV4 {
        self.server
    }

    #[must_use]
    pub const fn command_timeout(&self) -> Duration {
        self.command_timeout
    }

    #[must_use]
    pub const fn output_limit(&self) -> usize {
        self.output_limit
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UsbipBusId(String);

impl UsbipBusId {
    pub fn from_viiper(bus_id: u32, device_id: &str) -> Result<Self, UsbipWin2Error> {
        if bus_id == 0
            || device_id.is_empty()
            || device_id.len() > 10
            || !device_id.bytes().all(|value| value.is_ascii_digit())
            || device_id.bytes().all(|value| value == b'0')
        {
            return Err(UsbipWin2Error::InvalidViiperDeviceIdentity {
                bus_id,
                device_id: device_id.to_owned(),
            });
        }
        Self::parse(&format!("{bus_id}-{device_id}"))
    }

    fn parse(value: &str) -> Result<Self, UsbipWin2Error> {
        if value.is_empty()
            || value.len() > MAX_BUS_ID_BYTES
            || value
                .bytes()
                .any(|byte| !byte.is_ascii_digit() && byte != b'-')
        {
            return Err(UsbipWin2Error::InvalidBusId(value.to_owned()));
        }
        let Some((bus, device)) = value.split_once('-') else {
            return Err(UsbipWin2Error::InvalidBusId(value.to_owned()));
        };
        if bus.is_empty()
            || device.is_empty()
            || device.contains('-')
            || bus.bytes().all(|byte| byte == b'0')
            || device.bytes().all(|byte| byte == b'0')
        {
            return Err(UsbipWin2Error::InvalidBusId(value.to_owned()));
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for UsbipBusId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsbipExportedDevice {
    bus_id: UsbipBusId,
    vendor_id: u16,
    product_id: u16,
    label: String,
}

impl UsbipExportedDevice {
    #[must_use]
    pub fn bus_id(&self) -> &UsbipBusId {
        &self.bus_id
    }

    #[must_use]
    pub const fn vendor_id(&self) -> u16 {
        self.vendor_id
    }

    #[must_use]
    pub const fn product_id(&self) -> u16 {
        self.product_id
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Clone)]
pub struct UsbipWin2Client {
    config: UsbipWin2Config,
    executor: Arc<dyn UsbipCommandExecutor>,
}

impl UsbipWin2Client {
    pub fn new(config: UsbipWin2Config) -> Self {
        Self {
            config,
            executor: Arc::new(StdUsbipCommandExecutor),
        }
    }

    pub fn probe(&self) -> Result<(), UsbipWin2Error> {
        let output = self.run(CommandKind::Version)?;
        let actual = output.stdout.trim();
        if actual != PINNED_USBIP_WIN2_VERSION {
            return Err(UsbipWin2Error::UnsupportedVersion(actual.to_owned()));
        }
        Ok(())
    }

    pub fn list_xbox360(&self, bus_id: &UsbipBusId) -> Result<UsbipExportedDevice, UsbipWin2Error> {
        self.list_controller(bus_id, UsbipControllerKind::Xbox360)
    }

    pub fn list_dualshock4(
        &self,
        bus_id: &UsbipBusId,
    ) -> Result<UsbipExportedDevice, UsbipWin2Error> {
        self.list_controller(bus_id, UsbipControllerKind::DualShock4)
    }

    fn list_controller(
        &self,
        bus_id: &UsbipBusId,
        kind: UsbipControllerKind,
    ) -> Result<UsbipExportedDevice, UsbipWin2Error> {
        let devices = self.list_exports()?;
        let mut matches = devices
            .into_iter()
            .filter(|device| device.bus_id == *bus_id);
        let device = matches
            .next()
            .ok_or_else(|| UsbipWin2Error::ExportNotFound(bus_id.clone()))?;
        if matches.next().is_some() {
            return Err(UsbipWin2Error::DuplicateExport(bus_id.clone()));
        }
        if device.vendor_id != kind.vendor_id() || device.product_id != kind.product_id() {
            return Err(UsbipWin2Error::UnexpectedExportIdentity {
                bus_id: bus_id.clone(),
                vendor_id: device.vendor_id,
                product_id: device.product_id,
            });
        }
        Ok(device)
    }

    /// Returns only exact Xbox 360 exports from a bounded, read-only list.
    ///
    /// This method never attaches a device. It intentionally omits unrelated
    /// exports so a local status surface does not receive a generic USB
    /// inventory. Multiple matches are retained for the caller to reject as
    /// ambiguous rather than selecting one implicitly.
    pub fn list_xbox360_exports(&self) -> Result<Vec<UsbipExportedDevice>, UsbipWin2Error> {
        self.list_controller_exports(UsbipControllerKind::Xbox360)
    }

    /// Returns only exact DualShock 4 `054c:09cc` exports without attaching.
    pub fn list_dualshock4_exports(&self) -> Result<Vec<UsbipExportedDevice>, UsbipWin2Error> {
        self.list_controller_exports(UsbipControllerKind::DualShock4)
    }

    fn list_controller_exports(
        &self,
        kind: UsbipControllerKind,
    ) -> Result<Vec<UsbipExportedDevice>, UsbipWin2Error> {
        Ok(self
            .list_exports()?
            .into_iter()
            .filter(|device| {
                device.vendor_id == kind.vendor_id() && device.product_id == kind.product_id()
            })
            .collect())
    }

    /// Attaches exactly one pre-listed VIIPER Xbox 360 export.
    ///
    /// `--once` disables usbip-win2 background retry/persistence. The returned
    /// value owns the reported hub port and must be explicitly stopped; Drop
    /// deliberately performs no process I/O.
    pub fn attach_xbox360_once(
        &self,
        _deployment_verified: UsbipWin2DeploymentVerified,
        bus_id: UsbipBusId,
    ) -> Result<UsbipOwnedAttachment, UsbipWin2Error> {
        self.attach_controller_once(bus_id, UsbipControllerKind::Xbox360)
    }

    pub fn attach_dualshock4_once(
        &self,
        _deployment_verified: UsbipWin2DeploymentVerified,
        bus_id: UsbipBusId,
    ) -> Result<UsbipOwnedAttachment, UsbipWin2Error> {
        self.attach_controller_once(bus_id, UsbipControllerKind::DualShock4)
    }

    fn attach_controller_once(
        &self,
        bus_id: UsbipBusId,
        kind: UsbipControllerKind,
    ) -> Result<UsbipOwnedAttachment, UsbipWin2Error> {
        self.list_controller(&bus_id, kind)?;
        let output = self.run(CommandKind::AttachOnce(&bus_id))?;
        let port = parse_attachment_port(&output.stdout)?;
        let mut attachment = UsbipOwnedAttachment {
            client: self.clone(),
            bus_id,
            kind,
            port,
            attached: true,
        };
        if let Err(error) = attachment.verify_present() {
            let cleanup = attachment.stop().err().map(|error| error.to_string());
            return Err(UsbipWin2Error::AttachmentVerificationFailed {
                port,
                detail: error.to_string(),
                cleanup,
            });
        }
        Ok(attachment)
    }

    fn run(&self, kind: CommandKind<'_>) -> Result<CommandOutput, UsbipWin2Error> {
        let operation = kind.operation();
        let arguments = kind.arguments(self.config.server);
        self.executor.run(&self.config, operation, &arguments)
    }

    fn list_exports(&self) -> Result<Vec<UsbipExportedDevice>, UsbipWin2Error> {
        self.probe()?;
        let output = self.run(CommandKind::List)?;
        parse_exported_devices(&output.stdout)
    }
}

impl fmt::Debug for UsbipWin2Client {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UsbipWin2Client")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct UsbipOwnedAttachment {
    client: UsbipWin2Client,
    bus_id: UsbipBusId,
    kind: UsbipControllerKind,
    port: u8,
    attached: bool,
}

impl UsbipOwnedAttachment {
    #[must_use]
    pub fn bus_id(&self) -> &UsbipBusId {
        &self.bus_id
    }

    #[must_use]
    pub const fn controller_kind(&self) -> UsbipControllerKind {
        self.kind
    }

    #[must_use]
    pub const fn port(&self) -> u8 {
        self.port
    }

    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
    }

    /// Confirms that the exact owned port still resolves to the expected
    /// loopback server, VIIPER bus ID and selected controller VID:PID. This is read-only
    /// and never converts an arbitrary imported port into ownership.
    pub fn verify_present(&self) -> Result<(), UsbipWin2Error> {
        if !self.attached {
            return Err(UsbipWin2Error::AttachmentNotPresent {
                port: self.port,
                bus_id: self.bus_id.clone(),
            });
        }
        let output = self.client.run(CommandKind::Port(self.port))?;
        validate_owned_attachment(
            &output.stdout,
            self.client.config.server(),
            self.port,
            &self.bus_id,
            self.kind,
        )
    }

    pub fn stop(&mut self) -> Result<(), UsbipWin2Error> {
        if !self.attached {
            return Ok(());
        }
        self.client.run(CommandKind::Detach(self.port))?;
        self.attached = false;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UsbipWin2Error {
    ExecutablePathNotAbsolute(PathBuf),
    UnexpectedExecutableName(PathBuf),
    NonLoopbackServer(SocketAddrV4),
    InvalidServerPort,
    InvalidCommandTimeout(Duration),
    InvalidOutputLimit {
        actual: usize,
        maximum: usize,
    },
    InvalidViiperDeviceIdentity {
        bus_id: u32,
        device_id: String,
    },
    InvalidBusId(String),
    SpawnFailed(String),
    WaitFailed(String),
    CommandTimedOut(&'static str),
    OutputReadFailed(String),
    OutputReaderPanicked,
    OutputTooLarge {
        stream: &'static str,
        maximum: usize,
    },
    InvalidUtf8(&'static str),
    CommandFailed {
        operation: &'static str,
        exit_code: Option<i32>,
        detail: String,
    },
    UnsupportedVersion(String),
    InvalidExportLine(String),
    ExportNotFound(UsbipBusId),
    DuplicateExport(UsbipBusId),
    UnexpectedExportIdentity {
        bus_id: UsbipBusId,
        vendor_id: u16,
        product_id: u16,
    },
    InvalidAttachmentPort(String),
    AttachmentNotPresent {
        port: u8,
        bus_id: UsbipBusId,
    },
    InvalidAttachmentStatus {
        port: u8,
        bus_id: UsbipBusId,
    },
    AttachmentVerificationFailed {
        port: u8,
        detail: String,
        cleanup: Option<String>,
    },
}

impl Display for UsbipWin2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExecutablePathNotAbsolute(path) => {
                write!(
                    formatter,
                    "usbip.exe path must be absolute: {}",
                    path.display()
                )
            }
            Self::UnexpectedExecutableName(path) => write!(
                formatter,
                "USB/IP executable must be named usbip.exe: {}",
                path.display()
            ),
            Self::NonLoopbackServer(server) => {
                write!(
                    formatter,
                    "USB/IP server must use explicit IPv4 loopback: {server}"
                )
            }
            Self::InvalidServerPort => formatter.write_str("USB/IP server port must be non-zero"),
            Self::InvalidCommandTimeout(timeout) => write!(
                formatter,
                "USB/IP command timeout must be within 1ns..={MAX_USBIP_COMMAND_TIMEOUT:?}: {timeout:?}"
            ),
            Self::InvalidOutputLimit { actual, maximum } => write!(
                formatter,
                "USB/IP output limit must be within 1..={maximum} bytes: {actual}"
            ),
            Self::InvalidViiperDeviceIdentity { bus_id, device_id } => write!(
                formatter,
                "invalid VIIPER bus/device identity: {bus_id}/{device_id}"
            ),
            Self::InvalidBusId(value) => write!(formatter, "invalid VIIPER USB/IP bus ID: {value}"),
            Self::SpawnFailed(detail) => write!(formatter, "could not start usbip.exe: {detail}"),
            Self::WaitFailed(detail) => write!(formatter, "could not wait for usbip.exe: {detail}"),
            Self::CommandTimedOut(operation) => {
                write!(formatter, "usbip.exe {operation} timed out")
            }
            Self::OutputReadFailed(detail) => {
                write!(formatter, "could not read usbip.exe output: {detail}")
            }
            Self::OutputReaderPanicked => formatter.write_str("usbip.exe output reader panicked"),
            Self::OutputTooLarge { stream, maximum } => {
                write!(formatter, "usbip.exe {stream} exceeded {maximum} bytes")
            }
            Self::InvalidUtf8(stream) => {
                write!(formatter, "usbip.exe {stream} is not valid UTF-8")
            }
            Self::CommandFailed {
                operation,
                exit_code,
                detail,
            } => write!(
                formatter,
                "usbip.exe {operation} failed with exit code {exit_code:?}: {detail}"
            ),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported usbip-win2 version: {version}")
            }
            Self::InvalidExportLine(line) => {
                write!(formatter, "invalid usbip-win2 export line: {line}")
            }
            Self::ExportNotFound(bus_id) => {
                write!(formatter, "USB/IP export {bus_id} was not found")
            }
            Self::DuplicateExport(bus_id) => {
                write!(formatter, "USB/IP export {bus_id} appeared more than once")
            }
            Self::UnexpectedExportIdentity {
                bus_id,
                vendor_id,
                product_id,
            } => write!(
                formatter,
                "USB/IP export {bus_id} has unexpected VID:PID {vendor_id:04x}:{product_id:04x}"
            ),
            Self::InvalidAttachmentPort(value) => {
                write!(
                    formatter,
                    "invalid usbip-win2 terse attachment port: {value}"
                )
            }
            Self::AttachmentNotPresent { port, bus_id } => write!(
                formatter,
                "owned USB/IP port {port} for export {bus_id} is not attached"
            ),
            Self::InvalidAttachmentStatus { port, bus_id } => write!(
                formatter,
                "owned USB/IP port {port} does not match controller export {bus_id}"
            ),
            Self::AttachmentVerificationFailed {
                port,
                detail,
                cleanup,
            } => {
                write!(
                    formatter,
                    "new USB/IP attachment on port {port} failed verification: {detail}"
                )?;
                if let Some(cleanup) = cleanup {
                    write!(formatter, "; exact-port cleanup also failed: {cleanup}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for UsbipWin2Error {}

enum CommandKind<'a> {
    Version,
    List,
    AttachOnce(&'a UsbipBusId),
    Port(u8),
    Detach(u8),
}

impl CommandKind<'_> {
    const fn operation(&self) -> &'static str {
        match self {
            Self::Version => "version probe",
            Self::List => "export list",
            Self::AttachOnce(_) => "one-shot attach",
            Self::Port(_) => "owned-port status",
            Self::Detach(_) => "detach",
        }
    }

    fn arguments(&self, server: SocketAddrV4) -> Vec<OsString> {
        match self {
            Self::Version => vec!["--version".into()],
            Self::List => vec![
                "--tcp-port".into(),
                server.port().to_string().into(),
                "list".into(),
                "--remote".into(),
                server.ip().to_string().into(),
            ],
            Self::AttachOnce(bus_id) => vec![
                "--tcp-port".into(),
                server.port().to_string().into(),
                "attach".into(),
                "--remote".into(),
                server.ip().to_string().into(),
                "--bus-id".into(),
                bus_id.as_str().into(),
                "--terse".into(),
                "--once".into(),
            ],
            Self::Port(port) => vec!["port".into(), port.to_string().into()],
            Self::Detach(port) => vec!["detach".into(), "--port".into(), port.to_string().into()],
        }
    }
}

struct CommandOutput {
    stdout: String,
}

trait UsbipCommandExecutor: Send + Sync {
    fn run(
        &self,
        config: &UsbipWin2Config,
        operation: &'static str,
        arguments: &[OsString],
    ) -> Result<CommandOutput, UsbipWin2Error>;
}

struct StdUsbipCommandExecutor;

impl UsbipCommandExecutor for StdUsbipCommandExecutor {
    fn run(
        &self,
        config: &UsbipWin2Config,
        operation: &'static str,
        arguments: &[OsString],
    ) -> Result<CommandOutput, UsbipWin2Error> {
        run_bounded_command(config, operation, arguments)
    }
}

struct DrainedOutput {
    bytes: Vec<u8>,
    exceeded: bool,
}

fn run_bounded_command(
    config: &UsbipWin2Config,
    operation: &'static str,
    arguments: &[OsString],
) -> Result<CommandOutput, UsbipWin2Error> {
    let mut command = Command::new(&config.executable);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = CREATE_NO_WINDOW;

    let mut child = command
        .spawn()
        .map_err(|error| UsbipWin2Error::SpawnFailed(error.to_string()))?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(UsbipWin2Error::SpawnFailed(
            "stdout pipe is missing".to_owned(),
        ));
    };
    let Some(stderr) = child.stderr.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(UsbipWin2Error::SpawnFailed(
            "stderr pipe is missing".to_owned(),
        ));
    };
    let output_limit = config.output_limit;
    let stdout_reader = match thread::Builder::new()
        .name("capyio-usbip-stdout".to_owned())
        .spawn(move || drain_bounded(stdout, output_limit))
    {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(UsbipWin2Error::SpawnFailed(error.to_string()));
        }
    };
    let stderr_reader = match thread::Builder::new()
        .name("capyio-usbip-stderr".to_owned())
        .spawn(move || drain_bounded(stderr, output_limit))
    {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_output(stdout_reader);
            return Err(UsbipWin2Error::SpawnFailed(error.to_string()));
        }
    };

    let deadline = Instant::now() + config.command_timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_output(stdout_reader);
                let _ = join_output(stderr_reader);
                return Err(UsbipWin2Error::CommandTimedOut(operation));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_output(stdout_reader);
                let _ = join_output(stderr_reader);
                return Err(UsbipWin2Error::WaitFailed(error.to_string()));
            }
        }
    };

    let stdout = join_output(stdout_reader)?;
    let stderr = join_output(stderr_reader)?;
    if stdout.exceeded {
        return Err(UsbipWin2Error::OutputTooLarge {
            stream: "stdout",
            maximum: config.output_limit,
        });
    }
    if stderr.exceeded {
        return Err(UsbipWin2Error::OutputTooLarge {
            stream: "stderr",
            maximum: config.output_limit,
        });
    }
    let stdout =
        String::from_utf8(stdout.bytes).map_err(|_| UsbipWin2Error::InvalidUtf8("stdout"))?;
    let stderr =
        String::from_utf8(stderr.bytes).map_err(|_| UsbipWin2Error::InvalidUtf8("stderr"))?;
    if !status.success() {
        return Err(UsbipWin2Error::CommandFailed {
            operation,
            exit_code: status.code(),
            detail: stderr.trim().to_owned(),
        });
    }
    Ok(CommandOutput { stdout })
}

fn drain_bounded(mut reader: impl Read, output_limit: usize) -> Result<DrainedOutput, io::Error> {
    let mut retained = Vec::with_capacity(output_limit.min(4096));
    let mut exceeded = false;
    let mut buffer = [0_u8; 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = output_limit.saturating_sub(retained.len());
        let retain = remaining.min(count);
        retained.extend_from_slice(&buffer[..retain]);
        exceeded |= retain < count;
    }
    Ok(DrainedOutput {
        bytes: retained,
        exceeded,
    })
}

fn join_output(
    reader: thread::JoinHandle<Result<DrainedOutput, io::Error>>,
) -> Result<DrainedOutput, UsbipWin2Error> {
    reader
        .join()
        .map_err(|_| UsbipWin2Error::OutputReaderPanicked)?
        .map_err(|error| UsbipWin2Error::OutputReadFailed(error.to_string()))
}

fn parse_exported_devices(output: &str) -> Result<Vec<UsbipExportedDevice>, UsbipWin2Error> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        let Some((bus_text, remainder)) = trimmed.split_once(':') else {
            continue;
        };
        let bus_text = bus_text.trim();
        if !bus_text
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let bus_id = UsbipBusId::parse(bus_text)
            .map_err(|_| UsbipWin2Error::InvalidExportLine(trimmed.to_owned()))?;
        let Some((label, identity)) = remainder.rsplit_once('(') else {
            return Err(UsbipWin2Error::InvalidExportLine(trimmed.to_owned()));
        };
        let identity = identity
            .strip_suffix(')')
            .ok_or_else(|| UsbipWin2Error::InvalidExportLine(trimmed.to_owned()))?;
        let Some((vendor, product)) = identity.split_once(':') else {
            return Err(UsbipWin2Error::InvalidExportLine(trimmed.to_owned()));
        };
        if vendor.len() != 4
            || product.len() != 4
            || !vendor.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !product.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(UsbipWin2Error::InvalidExportLine(trimmed.to_owned()));
        }
        let label = label.trim().trim_end_matches(':').trim();
        if label.is_empty()
            || label.len() > MAX_DEVICE_LABEL_BYTES
            || label.chars().any(char::is_control)
        {
            return Err(UsbipWin2Error::InvalidExportLine(trimmed.to_owned()));
        }
        devices.push(UsbipExportedDevice {
            bus_id,
            vendor_id: u16::from_str_radix(vendor, 16)
                .map_err(|_| UsbipWin2Error::InvalidExportLine(trimmed.to_owned()))?,
            product_id: u16::from_str_radix(product, 16)
                .map_err(|_| UsbipWin2Error::InvalidExportLine(trimmed.to_owned()))?,
            label: label.to_owned(),
        });
    }
    Ok(devices)
}

fn parse_attachment_port(output: &str) -> Result<u8, UsbipWin2Error> {
    let value = output.trim();
    if value.is_empty() || value.lines().count() != 1 {
        return Err(UsbipWin2Error::InvalidAttachmentPort(value.to_owned()));
    }
    let port = value
        .parse::<u16>()
        .map_err(|_| UsbipWin2Error::InvalidAttachmentPort(value.to_owned()))?;
    u8::try_from(port)
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| UsbipWin2Error::InvalidAttachmentPort(value.to_owned()))
}

fn validate_owned_attachment(
    output: &str,
    server: SocketAddrV4,
    port: u8,
    bus_id: &UsbipBusId,
    kind: UsbipControllerKind,
) -> Result<(), UsbipWin2Error> {
    let expected_port = format!("Port {port:02}: device in use");
    let port_lines = output
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("Port "))
        .collect::<Vec<_>>();
    if port_lines.is_empty() {
        return Err(UsbipWin2Error::AttachmentNotPresent {
            port,
            bus_id: bus_id.clone(),
        });
    }
    let expected_remote = format!("-> usbip://{server}/{}", bus_id.as_str());
    let expected_identity = format!("({:04x}:{:04x})", kind.vendor_id(), kind.product_id());
    let valid = port_lines.len() == 1
        && port_lines[0].starts_with(&expected_port)
        && output.contains(&expected_identity)
        && output.lines().any(|line| line.trim() == expected_remote);
    if !valid {
        return Err(UsbipWin2Error::InvalidAttachmentStatus {
            port,
            bus_id: bus_id.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, io::Cursor, net::Ipv4Addr, sync::Mutex};

    const REAL_LIST_FIXTURE: &str = "Exportable USB devices\n======================\n    1-1    : Microsoft Corp. : Xbox360 Controller (045e:028e)\n           : /sys/devices/pci0000:00/usb1/1-1\n           : Vendor Specific Class/Vendor Specific Subclass/Vendor Specific Protocol (ff/ff/ff)\n           :  0 - Vendor Specific Class/?/? (ff/5d/01)\n";
    const REAL_PORT_FIXTURE: &str = "Imported USB devices\n====================\nPort 07: device in use at Full Speed(12Mbps)\n         Microsoft Corp. : Xbox360 Controller (045e:028e)\n           -> usbip://127.0.0.1:3241/1-1\n           -> remote bus/dev 001/001\n";
    const DS4_LIST_FIXTURE: &str = "Exportable USB devices\n======================\n    2-4    : Sony Interactive Entertainment : Wireless Controller (054c:09cc)\n           : /sys/devices/virtual/usb/2-4\n           : Human Interface Device (03/00/00)\n";
    const DS4_PORT_FIXTURE: &str = "Imported USB devices\n====================\nPort 03: device in use at Full Speed(12Mbps)\n         Sony Interactive Entertainment : Wireless Controller (054c:09cc)\n           -> usbip://127.0.0.1:3241/2-4\n           -> remote bus/dev 002/004\n";

    fn config() -> UsbipWin2Config {
        UsbipWin2Config::new(
            PathBuf::from(r"C:\Program Files\USBip\usbip.exe"),
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, 3241),
            Duration::from_secs(2),
            16 * 1024,
        )
        .unwrap()
    }

    #[test]
    fn config_is_absolute_loopback_bounded_and_pinned_to_executable_name() {
        let valid = config();
        assert_eq!(valid.server().port(), 3241);
        assert_eq!(valid.output_limit(), 16 * 1024);

        for invalid in [
            UsbipWin2Config::new(
                PathBuf::from("usbip.exe"),
                valid.server(),
                valid.command_timeout(),
                valid.output_limit(),
            ),
            UsbipWin2Config::new(
                PathBuf::from(r"C:\Program Files\USBip\other.exe"),
                valid.server(),
                valid.command_timeout(),
                valid.output_limit(),
            ),
            UsbipWin2Config::new(
                valid.executable().to_owned(),
                SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), 3241),
                valid.command_timeout(),
                valid.output_limit(),
            ),
            UsbipWin2Config::new(
                valid.executable().to_owned(),
                valid.server(),
                Duration::ZERO,
                valid.output_limit(),
            ),
            UsbipWin2Config::new(
                valid.executable().to_owned(),
                valid.server(),
                valid.command_timeout(),
                MAX_USBIP_OUTPUT_BYTES + 1,
            ),
        ] {
            assert!(invalid.is_err());
        }
    }

    #[test]
    fn viiper_identity_becomes_one_closed_usbip_bus_id() {
        assert_eq!(UsbipBusId::from_viiper(1, "1").unwrap().as_str(), "1-1");
        for (bus, device) in [(0, "1"), (1, ""), (1, "0"), (1, "1.2"), (1, "12345678901")] {
            assert!(UsbipBusId::from_viiper(bus, device).is_err());
        }
        for invalid in ["", "1", "1-", "-1", "0-1", "1-0", "1-1-1", "1.1-1"] {
            assert!(UsbipBusId::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn real_list_fixture_selects_exact_xbox360_identity() {
        let devices = parse_exported_devices(REAL_LIST_FIXTURE).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].bus_id().as_str(), "1-1");
        assert_eq!(devices[0].vendor_id(), USBIP_XBOX360_VENDOR_ID);
        assert_eq!(devices[0].product_id(), USBIP_XBOX360_PRODUCT_ID);
        assert_eq!(devices[0].label(), "Microsoft Corp. : Xbox360 Controller");

        for invalid in [
            REAL_LIST_FIXTURE.replace("(045e:028e)", "045e:028e"),
            REAL_LIST_FIXTURE.replace("(045e:028e)", "(45e:028e)"),
            REAL_LIST_FIXTURE.replace("1-1", "1.1-1"),
        ] {
            assert!(parse_exported_devices(&invalid).is_err());
        }
    }

    #[test]
    fn read_only_inventory_returns_only_exact_xbox360_exports() {
        let mixed =
            format!("{REAL_LIST_FIXTURE}    2-4    : Example Corp. : Other Device (1234:5678)\n");
        let executor = Arc::new(FixtureExecutor::new(["0.9.7.7\n", mixed.as_str()]));
        let client = UsbipWin2Client {
            config: config(),
            executor: executor.clone(),
        };

        let exports = client.list_xbox360_exports().unwrap();
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].bus_id().as_str(), "1-1");
        assert_eq!(exports[0].vendor_id(), USBIP_XBOX360_VENDOR_ID);
        assert_eq!(exports[0].product_id(), USBIP_XBOX360_PRODUCT_ID);

        let calls = executor.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], ["--version"].map(OsString::from));
        assert_eq!(
            calls[1],
            ["--tcp-port", "3241", "list", "--remote", "127.0.0.1"].map(OsString::from)
        );
    }

    #[test]
    fn dualshock4_inventory_and_owned_port_require_exact_identity() {
        let mixed = format!("{REAL_LIST_FIXTURE}{DS4_LIST_FIXTURE}");
        let executor = Arc::new(FixtureExecutor::new(["0.9.7.7\n", mixed.as_str()]));
        let client = UsbipWin2Client {
            config: config(),
            executor,
        };
        let exports = client.list_dualshock4_exports().unwrap();
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].bus_id().as_str(), "2-4");
        assert_eq!(exports[0].vendor_id(), USBIP_DS4_VENDOR_ID);
        assert_eq!(exports[0].product_id(), USBIP_DS4_PRODUCT_ID);

        let bus_id = UsbipBusId::from_viiper(2, "4").unwrap();
        validate_owned_attachment(
            DS4_PORT_FIXTURE,
            config().server(),
            3,
            &bus_id,
            UsbipControllerKind::DualShock4,
        )
        .unwrap();
        assert!(
            validate_owned_attachment(
                &DS4_PORT_FIXTURE.replace("(054c:09cc)", "(045e:028e)"),
                config().server(),
                3,
                &bus_id,
                UsbipControllerKind::DualShock4,
            )
            .is_err()
        );
    }

    #[test]
    fn command_arguments_never_use_shell_or_persistent_attach() {
        let server = config().server();
        let bus_id = UsbipBusId::from_viiper(7, "3").unwrap();
        assert_eq!(
            CommandKind::List.arguments(server),
            ["--tcp-port", "3241", "list", "--remote", "127.0.0.1"].map(OsString::from)
        );
        assert_eq!(
            CommandKind::AttachOnce(&bus_id).arguments(server),
            [
                "--tcp-port",
                "3241",
                "attach",
                "--remote",
                "127.0.0.1",
                "--bus-id",
                "7-3",
                "--terse",
                "--once",
            ]
            .map(OsString::from)
        );
        assert_eq!(
            CommandKind::Port(9).arguments(server),
            ["port", "9"].map(OsString::from)
        );
        assert_eq!(
            CommandKind::Detach(9).arguments(server),
            ["detach", "--port", "9"].map(OsString::from)
        );
    }

    #[test]
    fn terse_attach_port_is_single_bounded_nonzero_integer() {
        assert_eq!(parse_attachment_port("1\r\n").unwrap(), 1);
        assert_eq!(parse_attachment_port("255").unwrap(), 255);
        for invalid in ["", "0", "256", "1\n2", "port 1", "-1"] {
            assert!(
                parse_attachment_port(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn owned_port_status_requires_exact_port_server_bus_and_xbox_identity() {
        let bus_id = UsbipBusId::from_viiper(1, "1").unwrap();
        validate_owned_attachment(
            REAL_PORT_FIXTURE,
            config().server(),
            7,
            &bus_id,
            UsbipControllerKind::Xbox360,
        )
        .unwrap();
        for invalid in [
            REAL_PORT_FIXTURE.replace("Port 07", "Port 08"),
            REAL_PORT_FIXTURE.replace("(045e:028e)", "(1234:5678)"),
            REAL_PORT_FIXTURE.replace("/1-1", "/1-2"),
            REAL_PORT_FIXTURE.replace("127.0.0.1:3241", "127.0.0.1:4241"),
        ] {
            assert!(
                validate_owned_attachment(
                    &invalid,
                    config().server(),
                    7,
                    &bus_id,
                    UsbipControllerKind::Xbox360,
                )
                .is_err(),
                "accepted mismatched owned-port inventory"
            );
        }
        assert!(
            validate_owned_attachment(
                "",
                config().server(),
                7,
                &bus_id,
                UsbipControllerKind::Xbox360,
            )
            .is_err()
        );
    }

    #[test]
    fn output_drain_retains_a_fixed_prefix_and_continues_to_eof() {
        let exact = drain_bounded(Cursor::new(b"1234"), 4).unwrap();
        assert_eq!(exact.bytes, b"1234");
        assert!(!exact.exceeded);

        let oversized = drain_bounded(Cursor::new(b"123456"), 4).unwrap();
        assert_eq!(oversized.bytes, b"1234");
        assert!(oversized.exceeded);
    }

    #[test]
    fn owned_attachment_orders_probe_list_attach_once_and_exact_detach() {
        let executor = Arc::new(FixtureExecutor::new([
            "0.9.7.7\n",
            REAL_LIST_FIXTURE,
            "7\n",
            REAL_PORT_FIXTURE,
            REAL_PORT_FIXTURE,
            "port 7 is successfully detached\n",
        ]));
        let client = UsbipWin2Client {
            config: config(),
            executor: executor.clone(),
        };
        let mut attachment = client
            .attach_xbox360_once(
                UsbipWin2DeploymentVerified::confirmed_by_caller(),
                UsbipBusId::from_viiper(1, "1").unwrap(),
            )
            .unwrap();
        assert_eq!(attachment.port(), 7);
        assert!(attachment.is_attached());
        attachment.verify_present().unwrap();
        attachment.stop().unwrap();
        attachment.stop().unwrap();
        assert!(!attachment.is_attached());

        let calls = executor.calls.lock().unwrap();
        assert_eq!(calls.len(), 6);
        assert_eq!(calls[0], ["--version"].map(OsString::from));
        assert_eq!(
            calls[1],
            ["--tcp-port", "3241", "list", "--remote", "127.0.0.1"].map(OsString::from)
        );
        assert_eq!(
            calls[2],
            [
                "--tcp-port",
                "3241",
                "attach",
                "--remote",
                "127.0.0.1",
                "--bus-id",
                "1-1",
                "--terse",
                "--once",
            ]
            .map(OsString::from)
        );
        assert_eq!(calls[3], ["port", "7"].map(OsString::from));
        assert_eq!(calls[4], ["port", "7"].map(OsString::from));
        assert_eq!(calls[5], ["detach", "--port", "7"].map(OsString::from));
    }

    #[test]
    fn failed_post_attach_verification_attempts_exact_port_cleanup() {
        let executor = Arc::new(FixtureExecutor::new([
            "0.9.7.7\n",
            REAL_LIST_FIXTURE,
            "7\n",
            "",
            "port 7 is successfully detached\n",
        ]));
        let client = UsbipWin2Client {
            config: config(),
            executor: executor.clone(),
        };
        let error = client
            .attach_xbox360_once(
                UsbipWin2DeploymentVerified::confirmed_by_caller(),
                UsbipBusId::from_viiper(1, "1").unwrap(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            UsbipWin2Error::AttachmentVerificationFailed {
                port: 7,
                cleanup: None,
                ..
            }
        ));
        let calls = executor.calls.lock().unwrap();
        assert_eq!(calls.len(), 5);
        assert_eq!(calls[3], ["port", "7"].map(OsString::from));
        assert_eq!(calls[4], ["detach", "--port", "7"].map(OsString::from));
    }

    struct FixtureExecutor {
        outputs: Mutex<VecDeque<String>>,
        calls: Mutex<Vec<Vec<OsString>>>,
    }

    impl FixtureExecutor {
        fn new<const N: usize>(outputs: [&str; N]) -> Self {
            Self {
                outputs: Mutex::new(outputs.into_iter().map(str::to_owned).collect()),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl UsbipCommandExecutor for FixtureExecutor {
        fn run(
            &self,
            _config: &UsbipWin2Config,
            _operation: &'static str,
            arguments: &[OsString],
        ) -> Result<CommandOutput, UsbipWin2Error> {
            self.calls.lock().unwrap().push(arguments.to_vec());
            self.outputs
                .lock()
                .unwrap()
                .pop_front()
                .map(|stdout| CommandOutput { stdout })
                .ok_or_else(|| UsbipWin2Error::SpawnFailed("fixture exhausted".to_owned()))
        }
    }
}
