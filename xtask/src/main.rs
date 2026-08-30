use std::{
    collections::BTreeSet,
    env,
    ffi::{OsStr, OsString},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask")]
#[command(about = "CapyIO repository development commands")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Debug, Subcommand)]
enum Task {
    /// Reports required and optional development tools without modifying the host.
    Doctor,
    /// Formats Rust sources and, when available, runs frontend type checking.
    Fmt,
    /// Runs Rust static checks excluding the platform-dependent Tauri shell.
    Check,
    /// Runs Rust unit and integration tests excluding the Tauri shell.
    Test,
    /// Runs the local merge-gate approximation.
    Ci,
    /// Runs the deterministic CLI demo.
    Demo,
    /// Replays the deterministic IMU fixture into Panel and Recorder sinks.
    ImuDemo,
    /// Validates the normative documentation inventory and Requirement IDs.
    ValidateDocs,
    /// Validates the Adapter manifest schema and every committed manifest.
    ValidateManifests,
    /// Builds and supervises the finite Mock Source/Sink Sidecars.
    AdapterSmoke,
    /// Runs the Android node contract, Lint, and debug APK build without a device.
    AndroidCheck,
    /// Verifies one explicitly selected Android device using read-only ADB calls.
    AndroidDoctor {
        /// Exact serial from `adb devices`; never inferred from device order.
        #[arg(long)]
        serial: String,
    },
    /// Prints a sanitized, read-only Android baseline without writing an artifact.
    AndroidBaseline {
        /// Exact serial from `adb devices`; never inferred from device order.
        #[arg(long)]
        serial: String,
    },
    /// Writes a sanitized Android baseline below ignored `test-results/android/`.
    AndroidCollect {
        /// Exact serial from `adb devices`; never inferred from device order.
        #[arg(long)]
        serial: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = repository_root()?;
    env::set_current_dir(&root).context("change to repository root")?;

    match cli.command {
        Task::Doctor => doctor(),
        Task::Fmt => format_sources(),
        Task::Check => check(),
        Task::Test => test(),
        Task::Ci => ci(),
        Task::Demo => run("cargo", ["run", "--package", "capyio-node", "--", "demo"]),
        Task::ImuDemo => imu_demo(),
        Task::ValidateDocs => validate_docs(),
        Task::ValidateManifests => validate_manifests(),
        Task::AdapterSmoke => adapter_smoke(&root),
        Task::AndroidCheck => android_check(&root),
        Task::AndroidDoctor { serial } => android_doctor(&serial),
        Task::AndroidBaseline { serial } => android_baseline(&serial).map(|value| {
            println!("{value}");
        }),
        Task::AndroidCollect { serial } => android_collect(&root, &serial),
    }
}

fn android_check(root: &Path) -> Result<()> {
    let project_dir = root.join("platform/android");
    #[cfg(windows)]
    {
        let wrapper = project_dir.join("gradlew.bat");
        if !wrapper.is_file() {
            bail!("Android Gradle wrapper is missing: {}", wrapper.display());
        }
        run_path(
            &wrapper,
            [
                OsString::from("--project-dir"),
                project_dir.into_os_string(),
                OsString::from("capyioCheck"),
            ],
        )
    }

    #[cfg(not(windows))]
    {
        let wrapper = project_dir.join("gradlew");
        if !wrapper.is_file() {
            bail!("Android Gradle wrapper is missing: {}", wrapper.display());
        }
        run(
            "sh",
            [
                wrapper.into_os_string(),
                OsString::from("--project-dir"),
                project_dir.into_os_string(),
                OsString::from("capyioCheck"),
            ],
        )
    }
}

fn repository_root() -> Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .context("xtask is expected directly below the repository root")
}

fn doctor() -> Result<()> {
    println!("CapyIO environment doctor (read-only)\n");

    let required = [
        Tool::new("git", ["--version"]),
        Tool::new("rustc", ["--version"]),
        Tool::new("cargo", ["--version"]),
        Tool::new("node", ["--version"]),
        Tool::new("corepack", ["--version"]),
        Tool::new("python", ["--version"]),
    ];
    let optional = [
        Tool::new("pnpm", ["--version"]),
        Tool::new("adb", ["version"]),
        Tool::new("java", ["-version"]),
        Tool::new("msbuild", ["-version"]),
        Tool::new("windbg", ["-version"]),
    ];

    let mut missing_required = Vec::new();
    println!("required-now:");
    for tool in &required {
        if !print_tool(tool) {
            missing_required.push(tool.program);
        }
    }

    println!("\noptional-android:");
    for tool in &optional[..2] {
        print_tool(tool);
    }

    println!("\noptional-windows-native:");
    for tool in &optional[2..] {
        print_tool(tool);
    }

    println!("\nRepository checks:");
    for path in [
        "docs/PRODUCT_REQUIREMENTS.md",
        "docs/ARCHITECTURE.md",
        "protocol/proto/capyio/v1/control.proto",
        "apps/desktop/package.json",
    ] {
        let present = Path::new(path).is_file();
        println!("  {:<42} {}", path, if present { "OK" } else { "MISSING" });
        if !present {
            missing_required.push(path);
        }
    }

    if missing_required.is_empty() {
        println!("\nDoctor result: required bootstrap tools/files are present.");
        Ok(())
    } else {
        bail!(
            "doctor found missing required items: {}",
            missing_required.join(", ")
        )
    }
}

fn format_sources() -> Result<()> {
    run("cargo", ["fmt", "--all"])?;
    if command_available("pnpm") {
        run("pnpm", ["--filter", "@capyio/desktop", "typecheck"])?;
    } else {
        println!("pnpm is not available; frontend typecheck was skipped");
    }
    Ok(())
}

fn check() -> Result<()> {
    run(
        "cargo",
        [
            "check",
            "--workspace",
            "--exclude",
            "capyio-desktop",
            "--all-targets",
        ],
    )?;
    run(
        "cargo",
        [
            "clippy",
            "--workspace",
            "--exclude",
            "capyio-desktop",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )
}

fn test() -> Result<()> {
    run(
        "cargo",
        ["test", "--workspace", "--exclude", "capyio-desktop"],
    )
}

fn ci() -> Result<()> {
    run("cargo", ["fmt", "--all", "--", "--check"])?;
    check()?;
    test()?;
    imu_demo()?;
    validate_docs()?;
    validate_manifests()?;
    adapter_smoke(&repository_root()?)?;
    run("python", ["scripts/validate_repository.py"])?;

    if command_available("pnpm") {
        run("pnpm", ["--filter", "@capyio/desktop", "typecheck"])?;
        run("pnpm", ["--filter", "@capyio/desktop", "build"])?;
    } else {
        println!("pnpm is not available; frontend checks were skipped");
    }
    Ok(())
}

fn validate_docs() -> Result<()> {
    let required = [
        "README.md",
        "docs/PRODUCT_REQUIREMENTS.md",
        "docs/ARCHITECTURE.md",
        "docs/PROJECT_CHARTER.md",
        "docs/DOMAIN_MODEL.md",
        "docs/ADAPTER_MODEL.md",
        "docs/PORT_PROFILES.md",
        "docs/PROTOCOL.md",
        "docs/SECURITY_MODEL.md",
        "docs/TESTING.md",
        "docs/UX_MODEL.md",
        "docs/THIRD_PARTY_STRATEGY.md",
        "docs/ROADMAP.md",
        "docs/BACKLOG.md",
        "docs/BUILD_STATUS.md",
        "docs/REQUIREMENTS_TRACEABILITY.md",
    ];
    for path in required {
        if !Path::new(path).is_file() {
            bail!("required documentation is missing: {path}");
        }
    }
    run(
        "python",
        ["scripts/validate_repository.py", "--validate-docs"],
    )
}

fn validate_manifests() -> Result<()> {
    let schema_path = Path::new("protocol/schemas/adapter-manifest.schema.json");
    let schema: serde_json::Value = serde_json::from_slice(
        &fs::read(schema_path).context("read Adapter manifest JSON Schema")?,
    )
    .context("parse Adapter manifest JSON Schema")?;
    if schema["$schema"] != "https://json-schema.org/draft/2020-12/schema"
        || schema["properties"]["schema_version"]["const"].as_u64()
            != Some(u64::from(
                capyio_adapter_sdk::ADAPTER_MANIFEST_SCHEMA_VERSION,
            ))
    {
        bail!(
            "Adapter manifest JSON Schema does not declare supported version {}",
            capyio_adapter_sdk::ADAPTER_MANIFEST_SCHEMA_VERSION
        );
    }

    let mut paths = Vec::new();
    collect_named_files(Path::new("adapters"), "adapter.json", &mut paths)?;
    if paths.is_empty() {
        bail!("no Adapter manifests found");
    }
    paths.sort();
    for path in &paths {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let manifest = capyio_adapter_sdk::AdapterManifest::from_json(&bytes)
            .with_context(|| format!("validate {}", path.display()))?;
        println!("  OK  {} ({})", path.display(), manifest.id);
    }
    println!(
        "Adapter manifest validation: PASS ({} manifests)",
        paths.len()
    );
    Ok(())
}

fn collect_named_files(root: &Path, name: &str, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_named_files(&path, name, output)?;
        } else if path.file_name().is_some_and(|value| value == name) {
            output.push(path);
        }
    }
    Ok(())
}

fn adapter_smoke(root: &Path) -> Result<()> {
    run(
        "cargo",
        [
            "build",
            "--package",
            "capyio-mock-source",
            "--package",
            "capyio-mock-sink",
            "--package",
            "capyio-adapter-host",
            "--bins",
        ],
    )?;
    run(
        "cargo",
        [
            "test",
            "--package",
            "capyio-adapter-host",
            "--test",
            "crash_isolation",
        ],
    )?;

    let target = match env::var_os("CARGO_TARGET_DIR") {
        Some(directory) if Path::new(&directory).is_absolute() => PathBuf::from(directory),
        Some(directory) => root.join(directory),
        None => root.join("target"),
    };
    let executable = |name: &str| {
        target
            .join("debug")
            .join(format!("{name}{}", env::consts::EXE_SUFFIX))
    };
    let smoke = executable("capyio-adapter-smoke");
    let source = executable("capyio-mock-source");
    let sink = executable("capyio-mock-sink");
    run_path(
        &smoke,
        [
            source.as_os_str(),
            root.join("adapters/mock-source/adapter.json").as_os_str(),
            sink.as_os_str(),
            root.join("adapters/mock-sink/adapter.json").as_os_str(),
        ],
    )
}

fn imu_demo() -> Result<()> {
    run(
        "cargo",
        ["run", "--package", "capyio-node", "--", "imu-fixture-demo"],
    )
}

const MAX_ADB_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_ANDROID_FIELD_BYTES: usize = 256;
const MAX_SENSOR_LINES: usize = 256;
const MAX_SENSOR_LINE_BYTES: usize = 512;

fn android_doctor(serial: &str) -> Result<()> {
    let adb = find_adb()?;
    android_preflight(&adb, serial)?;
    println!("Android doctor: PASS");
    println!("  adb: {}", adb.display());
    println!("  target: explicit serial is online and authorized");
    println!("  policy: read-only inventory; no APK, permission or settings mutation");
    Ok(())
}

fn android_baseline(serial: &str) -> Result<String> {
    let adb = find_adb()?;
    android_preflight(&adb, serial)?;

    let manufacturer = adb_property(&adb, serial, "ro.product.manufacturer")?;
    let model = adb_property(&adb, serial, "ro.product.model")?;
    let device = adb_property(&adb, serial, "ro.product.device")?;
    let android_release = adb_property(&adb, serial, "ro.build.version.release")?;
    let api_level = adb_property(&adb, serial, "ro.build.version.sdk")?
        .parse::<u32>()
        .context("Android API level is not numeric")?;
    let security_patch = adb_property(&adb, serial, "ro.build.version.security_patch")?;
    let abi = adb_property(&adb, serial, "ro.product.cpu.abi")?;
    let display_size = adb_text(&adb, serial, ["shell", "wm", "size"])?;
    let sensor_dump = adb_text(&adb, serial, ["shell", "dumpsys", "sensorservice"])?;
    let sensors = sanitize_sensor_list(&sensor_dump);

    let baseline = serde_json::json!({
        "schema_version": 1,
        "collection": "read_only_android_baseline",
        "target": "explicit-device",
        "transport": if serial.contains(':') { "wireless" } else { "usb" },
        "manufacturer": sanitize_android_field("manufacturer", &manufacturer)?,
        "model": sanitize_android_field("model", &model)?,
        "device": sanitize_android_field("device", &device)?,
        "android_release": sanitize_android_field("android_release", &android_release)?,
        "api_level": api_level,
        "security_patch": sanitize_android_field("security_patch", &security_patch)?,
        "primary_abi": sanitize_android_field("primary_abi", &abi)?,
        "display_size": sanitize_android_field("display_size", display_size.trim())?,
        "sensor_inventory": sensors,
        "claims": {
            "adb_inventory_only": true,
            "live_capyio_stream": false,
            "apk_installed": false,
            "permissions_changed": false
        }
    });
    serde_json::to_string_pretty(&baseline).context("serialize Android baseline")
}

fn android_collect(root: &Path, serial: &str) -> Result<()> {
    let baseline = android_baseline(serial)?;
    let run_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_millis();
    let directory = root
        .join("test-results")
        .join("android")
        .join(run_id.to_string());
    let output = directory.join("baseline.json");
    if output.exists() {
        bail!(
            "refusing to overwrite existing evidence: {}",
            output.display()
        );
    }
    fs::create_dir_all(&directory)
        .with_context(|| format!("create evidence directory {}", directory.display()))?;
    fs::write(&output, format!("{baseline}\n"))
        .with_context(|| format!("write sanitized baseline {}", output.display()))?;
    println!("Android baseline: PASS");
    println!("  sanitized evidence: {}", output.display());
    Ok(())
}

fn find_adb() -> Result<PathBuf> {
    let executable = format!("adb{}", env::consts::EXE_SUFFIX);
    let mut candidates = Vec::new();
    for variable in ["ANDROID_SDK_ROOT", "ANDROID_HOME"] {
        if let Some(root) = env::var_os(variable) {
            candidates.push(PathBuf::from(root).join("platform-tools").join(&executable));
        }
    }
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Android")
                .join("Sdk")
                .join("platform-tools")
                .join(&executable),
        );
    }
    if let Some(path) = candidates.into_iter().find(|path| path.is_file()) {
        return Ok(path);
    }
    if platform_command("adb").arg("version").output().is_ok() {
        return Ok(PathBuf::from(executable));
    }
    bail!("Android Platform-Tools not found; set ANDROID_SDK_ROOT/ANDROID_HOME or add adb to PATH")
}

fn android_preflight(adb: &Path, serial: &str) -> Result<()> {
    validate_adb_serial(serial)?;
    bounded_command(adb, [OsString::from("version")])?;
    let devices = bounded_command(adb, [OsString::from("devices"), OsString::from("-l")])?;
    require_online_device(&devices, serial)
}

fn validate_adb_serial(serial: &str) -> Result<()> {
    if serial.is_empty()
        || serial.len() > MAX_ANDROID_FIELD_BYTES
        || serial.chars().any(char::is_whitespace)
        || serial.chars().any(char::is_control)
    {
        bail!("ADB serial must contain 1..={MAX_ANDROID_FIELD_BYTES} non-whitespace bytes");
    }
    Ok(())
}

fn require_online_device(devices: &str, serial: &str) -> Result<()> {
    let matches = devices
        .lines()
        .skip_while(|line| !line.starts_with("List of devices attached"))
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?, fields.next()?))
        })
        .filter(|(candidate, _)| *candidate == serial)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => bail!("explicit Android serial is not present in `adb devices -l`"),
        [(_, "device")] => Ok(()),
        [(_, state)] => bail!("explicit Android target is not usable: state={state}"),
        _ => bail!("explicit Android serial appears more than once; refusing ambiguous target"),
    }
}

fn adb_property(adb: &Path, serial: &str, property: &str) -> Result<String> {
    let value = adb_text(adb, serial, ["shell", "getprop", property])?;
    let value = value.trim();
    if value.is_empty() {
        bail!("Android property is empty: {property}");
    }
    Ok(value.to_owned())
}

fn adb_text<const N: usize>(adb: &Path, serial: &str, arguments: [&str; N]) -> Result<String> {
    let mut args = vec![OsString::from("-s"), OsString::from(serial)];
    args.extend(arguments.into_iter().map(OsString::from));
    bounded_command(adb, args)
}

fn bounded_command<I>(program: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let mut child = Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start read-only command: {}", program.display()))?;
    let stdout = child.stdout.take().context("capture command stdout")?;
    let stderr = child.stderr.take().context("capture command stderr")?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout));
    let stderr_reader = thread::spawn(move || read_bounded(stderr));
    let status = child.wait().context("wait for read-only command")?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader thread panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader thread panicked"))??;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        bail!("read-only command failed with {status}: {}", detail.trim());
    }
    String::from_utf8(stdout).context("read-only command returned non-UTF-8 stdout")
}

fn read_bounded(mut reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((MAX_ADB_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .context("read bounded command output")?;
    if bytes.len() > MAX_ADB_OUTPUT_BYTES {
        bail!("read-only command output exceeded {MAX_ADB_OUTPUT_BYTES} bytes");
    }
    Ok(bytes)
}

fn sanitize_android_field(label: &str, value: &str) -> Result<String> {
    if value.is_empty()
        || value.len() > MAX_ANDROID_FIELD_BYTES
        || value
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        bail!("Android {label} is empty, oversized or contains control characters");
    }
    Ok(value.replace(['\r', '\n'], " ").trim().to_owned())
}

fn sanitize_sensor_list(dump: &str) -> Vec<String> {
    let mut in_sensor_list = false;
    let mut sensors = BTreeSet::new();
    for line in dump.lines() {
        let trimmed = line.trim();
        if trimmed == "Sensor List:" {
            in_sensor_list = true;
            continue;
        }
        if in_sensor_list && trimmed.ends_with(':') {
            break;
        }
        if in_sensor_list
            && !trimmed.is_empty()
            && trimmed.starts_with("0x")
            && trimmed.contains(") ")
            && trimmed.contains(" | ")
            && trimmed.len() <= MAX_SENSOR_LINE_BYTES
            && !trimmed.chars().any(char::is_control)
        {
            sensors.insert(trimmed.to_owned());
            if sensors.len() == MAX_SENSOR_LINES {
                break;
            }
        }
    }
    sensors.into_iter().collect()
}

fn command_available(program: &str) -> bool {
    platform_command(program).arg("--version").output().is_ok()
}

fn platform_command(program: &str) -> Command {
    #[cfg(windows)]
    {
        if matches!(program, "corepack" | "pnpm") {
            return Command::new(format!("{program}.cmd"));
        }
    }

    Command::new(program)
}

fn run<I, S>(program: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<_> = args
        .into_iter()
        .map(|value| value.as_ref().to_owned())
        .collect();
    println!(
        "> {} {}",
        program,
        args.iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let status = platform_command(program)
        .args(&args)
        .status()
        .with_context(|| format!("start command: {program}"))?;
    require_success(program, status)
}

fn run_path<I, S>(program: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|value| value.as_ref().to_owned())
        .collect::<Vec<_>>();
    println!(
        "> {} {}",
        program.display(),
        args.iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let status = Command::new(program)
        .args(&args)
        .status()
        .with_context(|| format!("start command: {}", program.display()))?;
    require_success(&program.display().to_string(), status)
}

fn require_success(program: &str, status: ExitStatus) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        bail!("{program} exited with {status}")
    }
}

struct Tool<const N: usize> {
    program: &'static str,
    args: [&'static str; N],
}

impl<const N: usize> Tool<N> {
    const fn new(program: &'static str, args: [&'static str; N]) -> Self {
        Self { program, args }
    }
}

fn print_tool<const N: usize>(tool: &Tool<N>) -> bool {
    match platform_command(tool.program).args(tool.args).output() {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if text.is_empty() {
                text = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            }
            let first_line = text.lines().next().unwrap_or("available");
            println!("  {:<12} OK  {}", tool.program, first_line);
            true
        }
        Err(_) => {
            println!("  {:<12} MISSING", tool.program);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_target_must_be_explicit_and_online() {
        let devices = "List of devices attached\nphone-a\tdevice product:test\nphone-b\toffline\n";
        assert!(require_online_device(devices, "phone-a").is_ok());
        assert!(require_online_device(devices, "phone-b").is_err());
        assert!(require_online_device(devices, "missing").is_err());
        assert!(validate_adb_serial("").is_err());
        assert!(validate_adb_serial("bad serial").is_err());
    }

    #[test]
    fn sensor_sanitizer_retains_only_bounded_inventory_section() {
        let dump = "header\nSensor List:\n0x1) Accelerometer | Vendor | type: 1\n0x2) Gyroscope | Vendor | type: 4\nFusion States:\nprivate app connection\n";
        assert_eq!(
            sanitize_sensor_list(dump),
            vec![
                "0x1) Accelerometer | Vendor | type: 1".to_owned(),
                "0x2) Gyroscope | Vendor | type: 4".to_owned(),
            ]
        );
    }
}
