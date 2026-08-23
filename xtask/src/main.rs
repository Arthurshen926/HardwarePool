use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
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
    /// Validates the normative documentation inventory and Requirement IDs.
    ValidateDocs,
    /// Validates the Adapter manifest schema and every committed manifest.
    ValidateManifests,
    /// Builds and supervises the finite Mock Source/Sink Sidecars.
    AdapterSmoke,
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
        Task::ValidateDocs => validate_docs(),
        Task::ValidateManifests => validate_manifests(),
        Task::AdapterSmoke => adapter_smoke(&root),
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
