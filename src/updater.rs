use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::BaseDirs;

use crate::config::manager::ConfigManager;

pub const GITHUB_REPO_OWNER: &str = "GabroITA";
pub const GITHUB_REPO_NAME: &str = "RustBind";
pub const GITHUB_REPO_URL: &str = "https://github.com/GabroITA/RustBind";
pub const GITHUB_RELEASES_LATEST_URL: &str = "https://github.com/GabroITA/RustBind/releases/latest";
pub const GITHUB_LATEST_API_URL: &str =
    "https://api.github.com/repos/GabroITA/RustBind/releases/latest";

pub const UPDATE_CACHE_FILENAME: &str = "update_cache.toml";
const PREFERENCES_FILENAME: &str = "preferences.toml";

const ALLOWED_HTTPS_HOSTS: [&str; 5] = [
    "api.github.com",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "codeload.github.com",
];

/// Informazioni essenziali su una release GitHub pubblicata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub version: String,
    pub html_url: String,
    pub zip_url: Option<String>,
    pub sha256_url: Option<String>,
}

/// Esito del controllo aggiornamenti rispetto alla versione locale corrente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckOutcome {
    UpToDate {
        current_version: String,
        latest_version: Option<String>,
        html_url: String,
    },
    UpdateAvailable(ReleaseInfo),
}

/// Risultato di un'analisi URL HTTPS verso GitHub (host, percorso + query).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHttpsUrl {
    pub host: String,
    pub path_and_query: String,
}

/// Valida che l'URL sia `https://` verso un host GitHub consentito, senza credenziali né caratteri di controllo.
pub fn validate_allowed_https_url(url: &str) -> Result<ParsedHttpsUrl, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("URL non valido o contenente caratteri non ammessi".to_string());
    }
    let rest = trimmed
        .strip_prefix("https://")
        .ok_or_else(|| "Sono consentiti esclusivamente URL HTTPS".to_string())?;
    let (authority, path_and_query) = match rest.find(['/', '?']) {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') || authority.contains(':') {
        return Err("Autorita' URL non ammessa".to_string());
    }
    let host_lower = authority.to_ascii_lowercase();
    if !ALLOWED_HTTPS_HOSTS
        .iter()
        .any(|allowed| *allowed == host_lower)
    {
        return Err(format!(
            "Host esterno non consentito per gli aggiornamenti: {host_lower}"
        ));
    }
    let normalized_path = if path_and_query.starts_with('/') {
        path_and_query.to_string()
    } else {
        format!("/{path_and_query}")
    };
    Ok(ParsedHttpsUrl {
        host: host_lower,
        path_and_query: normalized_path,
    })
}

/// Analizza una versione semantica (`v0.1.0`, `0.2.3`) restituendo `((major, minor, patch), is_prerelease)`.
pub fn parse_semver_parts(raw: &str) -> Option<((u64, u64, u64), bool)> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let no_build = stripped.split('+').next().unwrap_or(stripped);
    let (core, is_pre) = match no_build.split_once('-') {
        Some((c, pre)) => (c, !pre.is_empty()),
        None => (no_build, false),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(((major, minor, patch), is_pre))
}

/// Restituisce `true` se `candidate` rappresenta una versione strettamente successiva a `current`.
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    let Some((cur_ver, cur_pre)) = parse_semver_parts(current) else {
        return false;
    };
    let Some((cand_ver, cand_pre)) = parse_semver_parts(candidate) else {
        return false;
    };
    if cand_ver != cur_ver {
        cand_ver > cur_ver
    } else {
        cur_pre && !cand_pre
    }
}

/// Normalizza una stringa versione rimuovendo l'eventuale prefisso `v`/`V`.
pub fn clean_version_str(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed)
        .to_string()
}

/// Risolve la directory locale dati non-roaming (`%LOCALAPPDATA%\RustBind`).
pub fn default_local_data_dir() -> Option<PathBuf> {
    let base_dirs = BaseDirs::new()?;
    Some(base_dirs.data_local_dir().join("RustBind"))
}

/// Percorso standard del file cache aggiornamenti (`%LOCALAPPDATA%\RustBind\update_cache.toml`).
pub fn default_update_cache_path() -> Option<PathBuf> {
    Some(default_local_data_dir()?.join(UPDATE_CACHE_FILENAME))
}

/// Legge da `%APPDATA%\RustBind\preferences.toml` l'impostazione `auto_check_updates` (default: `true`).
pub fn read_auto_check_updates_from_app_data() -> bool {
    let Ok(app_data_dir) = ConfigManager::default_app_data_dir() else {
        return true;
    };
    let prefs_path = app_data_dir.join(PREFERENCES_FILENAME);
    let Ok(content) = fs::read_to_string(prefs_path) else {
        return true;
    };
    parse_auto_check_updates_from_preferences_toml(&content)
}

/// Estrae `auto_check_updates` dal contenuto TOML di `preferences.toml` (default: `true`).
pub fn parse_auto_check_updates_from_preferences_toml(content: &str) -> bool {
    let Ok(doc) = toml::from_str::<toml::Value>(content) else {
        return true;
    };
    doc.get("auto_check_updates")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true)
}

/// Scrive atomicamente in cache l'aggiornamento rilevato per condivisione rapida tra daemon e UI.
pub fn write_update_cache(
    path: &Path,
    current_version: &str,
    info: &ReleaseInfo,
) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut table = toml::map::Map::new();
    table.insert(
        "checked_for_version".to_string(),
        toml::Value::String(clean_version_str(current_version)),
    );
    table.insert(
        "latest_version".to_string(),
        toml::Value::String(info.version.clone()),
    );
    table.insert(
        "tag_name".to_string(),
        toml::Value::String(info.tag_name.clone()),
    );
    table.insert(
        "html_url".to_string(),
        toml::Value::String(info.html_url.clone()),
    );
    if let Some(zip) = &info.zip_url {
        table.insert("zip_url".to_string(), toml::Value::String(zip.clone()));
    }
    if let Some(sha) = &info.sha256_url {
        table.insert("sha256_url".to_string(), toml::Value::String(sha.clone()));
    }
    let checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    table.insert(
        "checked_at_unix".to_string(),
        toml::Value::Integer(checked_at),
    );

    let body = toml::to_string_pretty(&toml::Value::Table(table)).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("toml.tmp.{}", std::process::id()));
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err.to_string());
    }
    Ok(())
}

/// Rimuove il file cache degli aggiornamenti se presente.
pub fn clear_update_cache(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Restituisce un eventuale aggiornamento valido dalla cache solo se risulta più recente di `current_version`.
pub fn read_cached_update_for(path: &Path, current_version: &str) -> Option<ReleaseInfo> {
    let raw = fs::read_to_string(path).ok()?;
    let doc: toml::Value = toml::from_str(&raw).ok()?;
    let checked_for = doc.get("checked_for_version")?.as_str()?;
    if clean_version_str(checked_for) != clean_version_str(current_version) {
        return None;
    }
    let version = doc.get("latest_version")?.as_str()?.to_string();
    let tag_name = doc.get("tag_name")?.as_str()?.to_string();
    let html_url = doc.get("html_url")?.as_str()?.to_string();
    if validate_allowed_https_url(&html_url).is_err() {
        return None;
    }
    if !is_newer_version(current_version, &version) {
        return None;
    }
    let zip_url = doc
        .get("zip_url")
        .and_then(toml::Value::as_str)
        .filter(|u| validate_allowed_https_url(u).is_ok())
        .map(str::to_string);
    let sha256_url = doc
        .get("sha256_url")
        .and_then(toml::Value::as_str)
        .filter(|u| validate_allowed_https_url(u).is_ok())
        .map(str::to_string);
    Some(ReleaseInfo {
        tag_name,
        version,
        html_url,
        zip_url,
        sha256_url,
    })
}

/// Estrae la riga corrispondente al file indicato da un manifest `*.sha256.txt`.
pub fn parse_sha256_manifest_for_file(manifest: &str, filename: &str) -> Option<String> {
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let hash = parts.next()?;
        let target = parts.next()?;
        let target_clean = target.trim_start_matches('*');
        let leaf = Path::new(target_clean)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(target_clean);
        if leaf.eq_ignore_ascii_case(filename)
            && hash.len() == 64
            && hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Parser JSON minimale pure-Rust per `/repos/<owner>/<repo>/releases/latest`
// ---------------------------------------------------------------------------

struct JsonCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonCursor<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        self.skip_ws();
        if self.bytes.get(self.pos).copied() == Some(expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("JSON non valido alla posizione {}", self.pos))
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            self.pos += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    if self.pos >= self.bytes.len() {
                        return Err("Escape JSON incompleto".to_string());
                    }
                    let esc = self.bytes[self.pos];
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            if self.pos + 4 > self.bytes.len() {
                                return Err("Escape \\u JSON incompleto".to_string());
                            }
                            let hex = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
                                .map_err(|_| "UTF-8 non valido in escape \\u".to_string())?;
                            let code = u16::from_str_radix(hex, 16)
                                .map_err(|_| "Codice esadecimale \\u non valido".to_string())?;
                            self.pos += 4;
                            if let Some(ch) = char::from_u32(u32::from(code)) {
                                out.push(ch);
                            }
                        }
                        other => out.push(other as char),
                    }
                }
                _ => {
                    let start = self.pos - 1;
                    let ch_len = utf8_char_len(b);
                    let end = (start + ch_len).min(self.bytes.len());
                    let s = std::str::from_utf8(&self.bytes[start..end])
                        .map_err(|_| "Sequenza UTF-8 non valida nel JSON".to_string())?;
                    out.push_str(s);
                    self.pos = end;
                }
            }
        }
        Err("Stringa JSON non terminata".to_string())
    }

    fn parse_bool(&mut self) -> Result<bool, String> {
        self.skip_ws();
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(true)
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(false)
        } else {
            Err("Valore booleano JSON non valido".to_string())
        }
    }

    fn skip_value(&mut self) -> Result<(), String> {
        let Some(first) = self.peek() else {
            return Err("Fine inattesa del JSON".to_string());
        };
        match first {
            b'"' => {
                let _ = self.parse_string()?;
                Ok(())
            }
            b'{' => {
                self.pos += 1;
                if self.peek() == Some(b'}') {
                    self.pos += 1;
                    return Ok(());
                }
                loop {
                    let _ = self.parse_string()?;
                    self.expect(b':')?;
                    self.skip_value()?;
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            return Ok(());
                        }
                        _ => return Err("Oggetto JSON malformato".to_string()),
                    }
                }
            }
            b'[' => {
                self.pos += 1;
                if self.peek() == Some(b']') {
                    self.pos += 1;
                    return Ok(());
                }
                loop {
                    self.skip_value()?;
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b']') => {
                            self.pos += 1;
                            return Ok(());
                        }
                        _ => return Err("Array JSON malformato".to_string()),
                    }
                }
            }
            b't' => {
                if self.bytes[self.pos..].starts_with(b"true") {
                    self.pos += 4;
                    Ok(())
                } else {
                    Err("Token JSON non valido".to_string())
                }
            }
            b'f' => {
                if self.bytes[self.pos..].starts_with(b"false") {
                    self.pos += 5;
                    Ok(())
                } else {
                    Err("Token JSON non valido".to_string())
                }
            }
            b'n' => {
                if self.bytes[self.pos..].starts_with(b"null") {
                    self.pos += 4;
                    Ok(())
                } else {
                    Err("Token JSON non valido".to_string())
                }
            }
            b'-' | b'0'..=b'9' => {
                while self.pos < self.bytes.len()
                    && matches!(
                        self.bytes[self.pos],
                        b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
                    )
                {
                    self.pos += 1;
                }
                Ok(())
            }
            _ => Err("Valore JSON sconosciuto".to_string()),
        }
    }

    fn parse_assets_array(&mut self) -> Result<Vec<(String, String)>, String> {
        self.expect(b'[')?;
        let mut assets = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(assets);
        }
        loop {
            self.expect(b'{')?;
            let mut name = None;
            let mut url = None;
            if self.peek() != Some(b'}') {
                loop {
                    let key = self.parse_string()?;
                    self.expect(b':')?;
                    match key.as_str() {
                        "name" => name = Some(self.parse_string()?),
                        "browser_download_url" => url = Some(self.parse_string()?),
                        _ => self.skip_value()?,
                    }
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            break;
                        }
                        _ => return Err("Asset JSON malformato".to_string()),
                    }
                }
            } else {
                self.pos += 1;
            }
            if let (Some(n), Some(u)) = (name, url) {
                assets.push((n, u));
            }
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err("Array assets JSON malformato".to_string()),
            }
        }
        Ok(assets)
    }
}

fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if (first_byte & 0xE0) == 0xC0 {
        2
    } else if (first_byte & 0xF0) == 0xE0 {
        3
    } else {
        4
    }
}

/// Estrae `ReleaseInfo` dalla risposta JSON di GitHub `/repos/<owner>/<repo>/releases/latest`.
pub fn parse_github_latest_release_json(json_str: &str) -> Result<ReleaseInfo, String> {
    let mut cursor = JsonCursor::new(json_str);
    cursor.expect(b'{')?;

    let mut tag_name: Option<String> = None;
    let mut html_url: Option<String> = None;
    let mut draft = false;
    let mut prerelease = false;
    let mut assets: Vec<(String, String)> = Vec::new();

    if cursor.peek() != Some(b'}') {
        loop {
            let key = cursor.parse_string()?;
            cursor.expect(b':')?;
            match key.as_str() {
                "tag_name" => tag_name = Some(cursor.parse_string()?),
                "html_url" => html_url = Some(cursor.parse_string()?),
                "draft" => draft = cursor.parse_bool()?,
                "prerelease" => prerelease = cursor.parse_bool()?,
                "assets" => assets = cursor.parse_assets_array()?,
                _ => cursor.skip_value()?,
            }
            match cursor.peek() {
                Some(b',') => cursor.pos += 1,
                Some(b'}') => {
                    break;
                }
                _ => return Err("Oggetto release GitHub malformato".to_string()),
            }
        }
    }

    if draft || prerelease {
        return Err("La release più recente restituita da GitHub non è stabile".to_string());
    }

    let tag_name = tag_name.ok_or_else(|| "Campo tag_name mancante nella release".to_string())?;
    let version = clean_version_str(&tag_name);
    if parse_semver_parts(&version).is_none() {
        return Err(format!("Formato versione release non valido: {tag_name}"));
    }

    let html_url = html_url.unwrap_or_else(|| GITHUB_RELEASES_LATEST_URL.to_string());
    validate_allowed_https_url(&html_url)?;

    let expected_zip = format!("rustbind-{version}-windows-x64.zip");
    let expected_sha = format!("rustbind-{version}-windows-x64.sha256.txt");

    let zip_url = assets
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(&expected_zip))
        .or_else(|| {
            assets.iter().find(|(name, _)| {
                let low = name.to_ascii_lowercase();
                low.ends_with("-windows-x64.zip")
            })
        })
        .map(|(_, url)| url.clone())
        .filter(|u| validate_allowed_https_url(u).is_ok());

    let sha256_url = assets
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(&expected_sha))
        .or_else(|| {
            assets.iter().find(|(name, _)| {
                let low = name.to_ascii_lowercase();
                low.ends_with("-windows-x64.sha256.txt")
            })
        })
        .map(|(_, url)| url.clone())
        .filter(|u| validate_allowed_https_url(u).is_ok());

    Ok(ReleaseInfo {
        tag_name,
        version,
        html_url,
        zip_url,
        sha256_url,
    })
}

// ---------------------------------------------------------------------------
// SHA-256 pure Rust (FIPS 180-4) per verifica integrità pacchetti scaricati
// ---------------------------------------------------------------------------

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

struct Sha256 {
    state: [u32; 8],
    bit_len: u64,
    buffer: [u8; 64],
    buf_len: usize,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            bit_len: 0,
            buffer: [0u8; 64],
            buf_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.buffer[self.buf_len] = byte;
            self.buf_len += 1;
            self.bit_len = self.bit_len.wrapping_add(8);
            if self.buf_len == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buf_len = 0;
            }
        }
    }

    fn finalize_hex(mut self) -> String {
        let bit_len = self.bit_len;
        self.buffer[self.buf_len] = 0x80;
        self.buf_len += 1;

        if self.buf_len > 56 {
            while self.buf_len < 64 {
                self.buffer[self.buf_len] = 0;
                self.buf_len += 1;
            }
            let block = self.buffer;
            self.compress(&block);
            self.buf_len = 0;
        }

        while self.buf_len < 56 {
            self.buffer[self.buf_len] = 0;
            self.buf_len += 1;
        }
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        self.compress(&block);

        let mut hex = String::with_capacity(64);
        for word in self.state {
            use std::fmt::Write;
            let _ = write!(&mut hex, "{word:08x}");
        }
        hex
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            let off = i * 4;
            *word =
                u32::from_be_bytes([block[off], block[off + 1], block[off + 2], block[off + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

/// Calcola il digest SHA-256 esadecimale minuscolo di un buffer in memoria.
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize_hex()
}

/// Calcola il digest SHA-256 esadecimale minuscolo di un file su disco.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        let read = file.read(&mut buf).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hasher.finalize_hex())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_comparison_and_prerelease_handling() {
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(!is_newer_version("0.1.0", "v0.1.0"));
        assert!(!is_newer_version("0.2.0", "v0.1.9"));
        assert!(is_newer_version("0.1.0", "v0.1.1"));
        assert!(is_newer_version("0.1.0", "v0.2.0"));
        assert!(is_newer_version("0.9.9", "v0.10.0"));
        assert!(is_newer_version("0.1.0-rc1", "0.1.0"));
        assert!(!is_newer_version("0.1.0", "0.1.0-rc1"));
    }

    #[test]
    fn test_validate_allowed_https_url_strict() {
        let ok = validate_allowed_https_url(
            "https://api.github.com/repos/GabroITA/RustBind/releases/latest",
        )
        .unwrap();
        assert_eq!(ok.host, "api.github.com");
        assert_eq!(
            ok.path_and_query,
            "/repos/GabroITA/RustBind/releases/latest"
        );

        assert!(validate_allowed_https_url("http://github.com/GabroITA/RustBind").is_err());
        assert!(validate_allowed_https_url("https://evil.example.com/a.zip").is_err());
        assert!(validate_allowed_https_url("https://user@github.com/a").is_err());
        assert!(validate_allowed_https_url("https://github.com/a\ncmd").is_err());
    }

    #[test]
    fn test_parse_github_latest_release_json_extracts_assets() {
        let sample = r#"{
          "url": "https://api.github.com/repos/GabroITA/RustBind/releases/1",
          "html_url": "https://github.com/GabroITA/RustBind/releases/tag/v0.2.0",
          "tag_name": "v0.2.0",
          "draft": false,
          "prerelease": false,
          "body": "Notes with \"escaped\" quotes\nand newlines.",
          "assets": [
            {
              "name": "rustbind-0.2.0-windows-x64.sha256.txt",
              "browser_download_url": "https://github.com/GabroITA/RustBind/releases/download/v0.2.0/rustbind-0.2.0-windows-x64.sha256.txt"
            },
            {
              "name": "rustbind-0.2.0-windows-x64.zip",
              "browser_download_url": "https://github.com/GabroITA/RustBind/releases/download/v0.2.0/rustbind-0.2.0-windows-x64.zip"
            }
          ]
        }"#;
        let info = parse_github_latest_release_json(sample).unwrap();
        assert_eq!(info.tag_name, "v0.2.0");
        assert_eq!(info.version, "0.2.0");
        assert_eq!(
            info.html_url,
            "https://github.com/GabroITA/RustBind/releases/tag/v0.2.0"
        );
        assert_eq!(
            info.zip_url.as_deref(),
            Some(
                "https://github.com/GabroITA/RustBind/releases/download/v0.2.0/rustbind-0.2.0-windows-x64.zip"
            )
        );
        assert_eq!(
            info.sha256_url.as_deref(),
            Some(
                "https://github.com/GabroITA/RustBind/releases/download/v0.2.0/rustbind-0.2.0-windows-x64.sha256.txt"
            )
        );
    }

    #[test]
    fn test_sha256_and_manifest_parsing() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let manifest = "\
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  rustbind-0.2.0-windows-x64.zip
1111111111111111111111111111111111111111111111111111111111111111 *rustbind.exe
";
        assert_eq!(
            parse_sha256_manifest_for_file(manifest, "rustbind-0.2.0-windows-x64.zip").as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            parse_sha256_manifest_for_file(manifest, "rustbind.exe").as_deref(),
            Some("1111111111111111111111111111111111111111111111111111111111111111")
        );
    }

    #[test]
    fn test_update_cache_roundtrip_and_version_invalidation() {
        let dir = std::env::temp_dir().join(format!("rustbind_upd_cache_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache_path = dir.join(UPDATE_CACHE_FILENAME);
        let info = ReleaseInfo {
            tag_name: "v0.2.0".to_string(),
            version: "0.2.0".to_string(),
            html_url: "https://github.com/GabroITA/RustBind/releases/tag/v0.2.0".to_string(),
            zip_url: Some(
                "https://github.com/GabroITA/RustBind/releases/download/v0.2.0/rustbind-0.2.0-windows-x64.zip"
                    .to_string(),
            ),
            sha256_url: None,
        };
        write_update_cache(&cache_path, "0.1.0", &info).unwrap();
        assert_eq!(
            read_cached_update_for(&cache_path, "0.1.0"),
            Some(info.clone())
        );
        // Se il programma si è già aggiornato a 0.2.0 la cache precedente non vale più
        assert_eq!(read_cached_update_for(&cache_path, "0.2.0"), None);
        clear_update_cache(&cache_path);
        assert!(!cache_path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
