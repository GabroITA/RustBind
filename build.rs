use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc_exe() -> Option<PathBuf> {
    if let Ok(status) = Command::new("rc.exe").arg("/?").output()
        && status.status.success()
    {
        return Some(PathBuf::from("rc.exe"));
    }

    let mut roots: Vec<PathBuf> = Vec::new();
    for env_key in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(base) = env::var(env_key) {
            roots.push(Path::new(&base).join("Windows Kits").join("10").join("bin"));
        }
    }

    let arch = if env::var("CARGO_CFG_TARGET_ARCH")
        .map(|a| a == "x86")
        .unwrap_or(false)
    {
        "x86"
    } else {
        "x64"
    };

    let mut sdk_versions: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let candidate = entry.path().join(arch).join("rc.exe");
            if candidate.is_file() {
                sdk_versions.push(candidate);
            }
        }
    }

    sdk_versions.sort();
    sdk_versions.pop()
}

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let manifest_path = PathBuf::from(manifest_dir);
    let rc_path = {
        let direct = manifest_path.join("assets").join("rustbind.rc");
        if direct.is_file() {
            direct
        } else {
            manifest_path.join("..").join("assets").join("rustbind.rc")
        }
    };
    let ico_path = rc_path.with_file_name("rustbind.ico");

    println!("cargo:rerun-if-changed={}", rc_path.display());
    println!("cargo:rerun-if-changed={}", ico_path.display());

    if !rc_path.is_file() || !ico_path.is_file() {
        return;
    }

    let Some(rc_exe) = find_rc_exe() else {
        println!("cargo:warning=rc.exe non trovato: icona eseguibile saltata");
        return;
    };
    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    let res_path = PathBuf::from(out_dir).join("rustbind.res");

    let status = Command::new(rc_exe)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_path)
        .arg(&rc_path)
        .status();

    match status {
        Ok(st) if st.success() && res_path.is_file() => {
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
        }
        Ok(st) => {
            println!("cargo:warning=rc.exe terminato con stato {st}: icona non incorporata");
        }
        Err(err) => {
            println!("cargo:warning=Impossibile eseguire rc.exe ({err}): icona non incorporata");
        }
    }
}
