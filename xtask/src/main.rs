mod sha256;
mod win32;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const REQUIRED_BINS: [&str; 3] = ["rustbind.exe", "rustbind-ui.exe", "rustbindctl.exe"];

#[derive(Debug, Clone, PartialEq, Eq)]
enum TaskCommand {
    Help,
    Package(PackageArgs),
    Install(InstallArgs),
    Uninstall(UninstallArgs),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageArgs {
    run_tests: bool,
    skip_build: bool,
    build_msi: bool,
    out_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallArgs {
    source_dir: Option<PathBuf>,
    install_dir: Option<PathBuf>,
    force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UninstallArgs {
    install_dir: Option<PathBuf>,
    force: bool,
}

fn usage() -> &'static str {
    "\
Uso: cargo xtask <comando> [opzioni]

Comandi:
  package     Compila e genera il pacchetto portatile Windows x64 (.zip + SHA-256 + MSI opzionale)
    --run-tests         Esegue cargo fmt --check, cargo clippy e cargo test --workspace prima del build
    --skip-build        Usa i binari gia' presenti in target/release
    --build-msi         Genera anche l'installer MSI tramite WiX Toolset v4+ (`wix`)
    --out-dir <DIR>     Directory di output (default: dist)

  install     Installa i 3 eseguibili per l'utente corrente (%LOCALAPPDATA%\\Programs\\RustBind) + Menu Start
    --source-dir <DIR>  Cartella sorgente dei binari (default: dist\\rustbind-<ver>-windows-x64 o target\\release)
    --install-dir <DIR> Cartella di destinazione personalizzata
    --force             Termina processi RustBind in esecuzione nella destinazione prima di copiare

  uninstall   Rimuove i binari installati e il collegamento nel Menu Start (preserva %APPDATA%\\RustBind)
    --install-dir <DIR> Cartella da disinstallare
    --force             Termina processi RustBind in esecuzione nella destinazione prima di rimuovere

  -h, --help  Mostra questo messaggio"
}

fn parse_args<I, S>(args: I) -> Result<TaskCommand, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _program = iter.next();

    let Some(sub) = iter.next() else {
        return Err("Comando mancante. Usa `cargo xtask --help`.".to_string());
    };

    match sub.as_ref() {
        "-h" | "--help" => Ok(TaskCommand::Help),
        "package" => {
            let mut opts = PackageArgs {
                run_tests: false,
                skip_build: false,
                build_msi: false,
                out_dir: PathBuf::from("dist"),
            };
            while let Some(arg) = iter.next() {
                match arg.as_ref() {
                    "--run-tests" => opts.run_tests = true,
                    "--skip-build" => opts.skip_build = true,
                    "--build-msi" => opts.build_msi = true,
                    "--out-dir" => {
                        let val = iter
                            .next()
                            .ok_or_else(|| "Valore mancante per --out-dir.".to_string())?;
                        opts.out_dir = PathBuf::from(val.as_ref());
                    }
                    "-h" | "--help" => return Ok(TaskCommand::Help),
                    other => {
                        return Err(format!("Opzione non riconosciuta per `package`: {other}"));
                    }
                }
            }
            Ok(TaskCommand::Package(opts))
        }
        "install" => {
            let mut opts = InstallArgs {
                source_dir: None,
                install_dir: None,
                force: false,
            };
            while let Some(arg) = iter.next() {
                match arg.as_ref() {
                    "--source-dir" => {
                        let val = iter
                            .next()
                            .ok_or_else(|| "Valore mancante per --source-dir.".to_string())?;
                        opts.source_dir = Some(PathBuf::from(val.as_ref()));
                    }
                    "--install-dir" => {
                        let val = iter
                            .next()
                            .ok_or_else(|| "Valore mancante per --install-dir.".to_string())?;
                        opts.install_dir = Some(PathBuf::from(val.as_ref()));
                    }
                    "--force" => opts.force = true,
                    "-h" | "--help" => return Ok(TaskCommand::Help),
                    other => {
                        return Err(format!("Opzione non riconosciuta per `install`: {other}"));
                    }
                }
            }
            Ok(TaskCommand::Install(opts))
        }
        "uninstall" => {
            let mut opts = UninstallArgs {
                install_dir: None,
                force: false,
            };
            while let Some(arg) = iter.next() {
                match arg.as_ref() {
                    "--install-dir" => {
                        let val = iter
                            .next()
                            .ok_or_else(|| "Valore mancante per --install-dir.".to_string())?;
                        opts.install_dir = Some(PathBuf::from(val.as_ref()));
                    }
                    "--force" => opts.force = true,
                    "-h" | "--help" => return Ok(TaskCommand::Help),
                    other => {
                        return Err(format!("Opzione non riconosciuta per `uninstall`: {other}"));
                    }
                }
            }
            Ok(TaskCommand::Uninstall(opts))
        }
        other => Err(format!("Comando sconosciuto: {other}")),
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolve_package_version(doc: &toml::Value, workspace_version: &str) -> Result<String, String> {
    let pkg = doc
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "Sezione [package] mancante nel Cargo.toml.".to_string())?;
    let ver = pkg
        .get("version")
        .ok_or_else(|| "Campo package.version mancante nel Cargo.toml.".to_string())?;

    if let Some(s) = ver.as_str() {
        return Ok(s.to_string());
    }
    if ver
        .as_table()
        .and_then(|t| t.get("workspace"))
        .and_then(toml::Value::as_bool)
        == Some(true)
    {
        return Ok(workspace_version.to_string());
    }
    Err("Formato package.version non valido.".to_string())
}

fn read_workspace_version(repo: &Path) -> Result<String, String> {
    let root_raw = fs::read_to_string(repo.join("Cargo.toml"))
        .map_err(|e| format!("Impossibile leggere Cargo.toml root: {e}"))?;
    let ui_raw = fs::read_to_string(repo.join("ui").join("Cargo.toml"))
        .map_err(|e| format!("Impossibile leggere ui/Cargo.toml: {e}"))?;

    let root_toml: toml::Value =
        toml::from_str(&root_raw).map_err(|e| format!("Cargo.toml non valido: {e}"))?;
    let ui_toml: toml::Value =
        toml::from_str(&ui_raw).map_err(|e| format!("ui/Cargo.toml non valido: {e}"))?;

    let ws_version = root_toml
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| "Campo [workspace.package].version mancante in Cargo.toml.".to_string())?;

    let root_ver = resolve_package_version(&root_toml, ws_version)?;
    let ui_ver = resolve_package_version(&ui_toml, ws_version)?;
    if root_ver != ws_version || ui_ver != ws_version {
        return Err(format!(
            "Versioni disallineate: workspace={ws_version}, rustbind={root_ver}, rustbind-ui={ui_ver}"
        ));
    }
    Ok(ws_version.to_string())
}

fn run_cmd(program: &str, args: &[&str], cwd: &Path) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|e| format!("Impossibile avviare `{program}`: {e}"))?;
    if !status.success() {
        return Err(format!(
            "`{program} {}` terminato con stato {status}.",
            args.join(" ")
        ));
    }
    Ok(())
}

fn run_package(repo: &Path, args: &PackageArgs) -> Result<(), String> {
    let version = read_workspace_version(repo)?;
    println!("RustBind workspace version: v{version}");

    if args.run_tests {
        println!("Esecuzione controlli (fmt, clippy, test)...");
        run_cmd("cargo", &["fmt", "--check"], repo)?;
        run_cmd(
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
            repo,
        )?;
        run_cmd("cargo", &["test", "--workspace"], repo)?;
    }

    if !args.skip_build {
        println!("Build release di rustbind, rustbind-ui e rustbindctl...");
        run_cmd("cargo", &["build", "--release"], repo)?;
    }

    let release_dir = repo.join("target").join("release");
    for bin in REQUIRED_BINS {
        let p = release_dir.join(bin);
        if !p.is_file() {
            return Err(format!(
                "Eseguibile mancante in target\\release: {}",
                p.display()
            ));
        }
    }

    let out_root = if args.out_dir.is_absolute() {
        args.out_dir.clone()
    } else {
        repo.join(&args.out_dir)
    };
    fs::create_dir_all(&out_root)
        .map_err(|e| format!("Creazione {} fallita: {e}", out_root.display()))?;

    let package_name = format!("rustbind-{version}-windows-x64");
    let stage_dir = out_root.join(&package_name);
    let zip_path = out_root.join(format!("{package_name}.zip"));
    let checksum_path = out_root.join(format!("{package_name}.sha256.txt"));

    if stage_dir.exists() {
        fs::remove_dir_all(&stage_dir)
            .map_err(|e| format!("Pulizia {} fallita: {e}", stage_dir.display()))?;
    }
    fs::create_dir_all(&stage_dir)
        .map_err(|e| format!("Creazione {} fallita: {e}", stage_dir.display()))?;

    for bin in REQUIRED_BINS {
        fs::copy(release_dir.join(bin), stage_dir.join(bin))
            .map_err(|e| format!("Copia di {bin} fallita: {e}"))?;
    }

    let readme = format!(
        "\
RustBind v{version} (Windows x64 Portable)
=========================================

Contenuto del pacchetto:
- rustbind.exe     Daemon residente nella tray di Windows
- rustbind-ui.exe  Interfaccia grafica di configurazione
- rustbindctl.exe  Utility CLI di controllo locale (status, suspend, resume, reload)

Avvio rapido:
1. Mantieni sempre tutti gli eseguibili nella stessa cartella: daemon e UI
   si avviano reciprocamente risolvendo il binario compagno dalla propria directory.
2. Avvia rustbind-ui.exe per configurare regole e gruppi (avvia automaticamente
   anche il daemon rustbind.exe se non ancora in esecuzione).
3. In alternativa avvia rustbind.exe direttamente nella tray di sistema e usa
   tasto destro -> \"Configura...\" per aprire l'interfaccia.

Percorso predefinito configurazione e log:
- Configurazione: %APPDATA%\\RustBind\\config.toml
- Log:            %LOCALAPPDATA%\\RustBind\\logs\\
"
    );
    fs::write(stage_dir.join("README.txt"), readme)
        .map_err(|e| format!("Scrittura README.txt fallita: {e}"))?;

    if zip_path.exists() {
        fs::remove_file(&zip_path)
            .map_err(|e| format!("Rimozione {} fallita: {e}", zip_path.display()))?;
    }

    let zip_arg = zip_path.to_string_lossy().into_owned();
    let stage_arg = stage_dir.to_string_lossy().into_owned();
    run_cmd(
        "tar.exe",
        &["-a", "-c", "-f", &zip_arg, "-C", &stage_arg, "."],
        repo,
    )?;

    let mut hash_targets: Vec<PathBuf> = vec![zip_path.clone()];
    for bin in REQUIRED_BINS {
        hash_targets.push(stage_dir.join(bin));
    }

    let mut checksum_lines = Vec::new();
    for target in &hash_targets {
        let digest = sha256::sha256_file(target)
            .map_err(|e| format!("Calcolo SHA-256 fallito per {}: {e}", target.display()))?;
        let leaf = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        checksum_lines.push(format!("{digest}  {leaf}"));
    }

    if args.build_msi {
        let wxs_path = repo.join("scripts").join("wix").join("RustBind.wxs");
        if !wxs_path.is_file() {
            return Err(format!("Sorgente WiX non trovato: {}", wxs_path.display()));
        }
        let msi_path = out_root.join(format!("{package_name}.msi"));
        let wxs_arg = wxs_path.to_string_lossy().into_owned();
        let ver_def = format!("Version={version}");
        let bin_def = format!("BinDir={}", stage_dir.display());
        let repo_def = format!("RepoDir={}", repo.display());
        let msi_arg = msi_path.to_string_lossy().into_owned();

        println!("Generazione installer MSI via WiX v4...");
        run_cmd(
            "wix",
            &[
                "build",
                &wxs_arg,
                "-ext",
                "WixToolset.UI.wixext",
                "-ext",
                "WixToolset.Util.wixext",
                "-arch",
                "x64",
                "-d",
                &ver_def,
                "-d",
                &bin_def,
                "-d",
                &repo_def,
                "-o",
                &msi_arg,
            ],
            repo,
        )
        .map_err(|e| {
            format!(
                "{e}\nAssicurati che WiX Toolset v4+ (`wix`) e le estensioni WixToolset.UI.wixext / WixToolset.Util.wixext siano installati."
            )
        })?;

        let msi_digest = sha256::sha256_file(&msi_path)
            .map_err(|e| format!("Calcolo SHA-256 MSI fallito: {e}"))?;
        let leaf = msi_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        checksum_lines.push(format!("{msi_digest}  {leaf}"));
        println!("Installer MSI creato: {}", msi_path.display());
    }

    fs::write(&checksum_path, format!("{}\n", checksum_lines.join("\n")))
        .map_err(|e| format!("Scrittura {} fallita: {e}", checksum_path.display()))?;

    println!();
    println!("Pacchetto portatile pronto:");
    println!("  Cartella : {}", stage_dir.display());
    println!("  Archivio : {}", zip_path.display());
    println!("  SHA-256  : {}", checksum_path.display());
    Ok(())
}

fn default_install_dir() -> Result<PathBuf, String> {
    let local_app_data = env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "Variabile d'ambiente LOCALAPPDATA non definita.".to_string())?;
    Ok(PathBuf::from(local_app_data)
        .join("Programs")
        .join("RustBind"))
}

fn default_start_menu_shortcut() -> Result<PathBuf, String> {
    let app_data = env::var_os("APPDATA")
        .ok_or_else(|| "Variabile d'ambiente APPDATA non definita.".to_string())?;
    Ok(PathBuf::from(app_data)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("RustBind.lnk"))
}

fn has_all_bins(dir: &Path) -> bool {
    REQUIRED_BINS.iter().all(|bin| dir.join(bin).is_file())
}

fn resolve_source_dir(repo: &Path, explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(dir) = explicit {
        if has_all_bins(dir) {
            return Ok(dir.to_path_buf());
        }
        return Err(format!(
            "Eseguibili richiesti mancanti in '{}'.",
            dir.display()
        ));
    }

    let version = read_workspace_version(repo)?;
    let candidates = [
        repo.join("dist")
            .join(format!("rustbind-{version}-windows-x64")),
        repo.join("target").join("release"),
    ];
    for candidate in candidates {
        if has_all_bins(&candidate) {
            return Ok(candidate);
        }
    }

    Err(
        "Nessuna cartella valida con rustbind.exe, rustbind-ui.exe e rustbindctl.exe trovata. \
         Esegui prima `cargo xtask package` o specifica `--source-dir`."
            .to_string(),
    )
}

fn run_install(repo: &Path, args: &InstallArgs) -> Result<(), String> {
    let source_dir = resolve_source_dir(repo, args.source_dir.as_deref())?;
    let install_dir = match &args.install_dir {
        Some(dir) => dir.clone(),
        None => default_install_dir()?,
    };
    let shortcut_path = default_start_menu_shortcut()?;

    win32::ensure_binaries_not_running(&install_dir, args.force)?;
    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Creazione {} fallita: {e}", install_dir.display()))?;

    for bin in REQUIRED_BINS {
        fs::copy(source_dir.join(bin), install_dir.join(bin))
            .map_err(|e| format!("Copia di {bin} fallita: {e}"))?;
    }

    win32::create_windows_shortcut(
        &shortcut_path,
        &install_dir.join("rustbind-ui.exe"),
        &install_dir,
        "RustBind - Configurazione hotkey e automazione input",
    )?;

    println!("RustBind installato in: {}", install_dir.display());
    println!("Collegamento Menu Start: {}", shortcut_path.display());
    Ok(())
}

fn run_uninstall(args: &UninstallArgs) -> Result<(), String> {
    let install_dir = match &args.install_dir {
        Some(dir) => dir.clone(),
        None => default_install_dir()?,
    };
    let shortcut_path = default_start_menu_shortcut()?;

    win32::ensure_binaries_not_running(&install_dir, args.force)?;

    if shortcut_path.exists() {
        fs::remove_file(&shortcut_path)
            .map_err(|e| format!("Rimozione {} fallita: {e}", shortcut_path.display()))?;
    }
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)
            .map_err(|e| format!("Rimozione {} fallita: {e}", install_dir.display()))?;
    }

    println!(
        "RustBind rimosso da '{}'. Eventuali configurazioni in %APPDATA%\\RustBind non sono state toccate.",
        install_dir.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    let cmd = match parse_args(env::args()) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Errore: {err}");
            return ExitCode::from(2);
        }
    };

    let repo = repo_root();
    let res = match cmd {
        TaskCommand::Help => {
            println!("{}", usage());
            Ok(())
        }
        TaskCommand::Package(ref args) => run_package(&repo, args),
        TaskCommand::Install(ref args) => run_install(&repo, args),
        TaskCommand::Uninstall(ref args) => run_uninstall(args),
    };

    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Errore: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_xtask_args() {
        assert_eq!(parse_args(["xtask", "--help"]), Ok(TaskCommand::Help));
        assert_eq!(
            parse_args([
                "xtask",
                "package",
                "--run-tests",
                "--build-msi",
                "--out-dir",
                "custom_dist"
            ]),
            Ok(TaskCommand::Package(PackageArgs {
                run_tests: true,
                skip_build: false,
                build_msi: true,
                out_dir: PathBuf::from("custom_dist"),
            }))
        );
        assert_eq!(
            parse_args(["xtask", "install", "--force"]),
            Ok(TaskCommand::Install(InstallArgs {
                source_dir: None,
                install_dir: None,
                force: true,
            }))
        );
        assert_eq!(
            parse_args(["xtask", "uninstall"]),
            Ok(TaskCommand::Uninstall(UninstallArgs {
                install_dir: None,
                force: false,
            }))
        );
        assert!(parse_args(["xtask", "unknown"]).is_err());
    }

    #[test]
    fn test_workspace_version_matches_manifest() {
        let ver = read_workspace_version(&repo_root()).expect("workspace version readable");
        assert!(!ver.is_empty());
        assert_eq!(ver, env!("CARGO_PKG_VERSION"));
    }
}
