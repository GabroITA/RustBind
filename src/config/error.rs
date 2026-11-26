use std::path::PathBuf;
use thiserror::Error;

/// Errori correlati al caricamento, parsing, validazione e persistenza della configurazione.
///
/// Tutti i messaggi di errore sono conformi alla policy di privacy di RustBind: non contengono
/// MAI frammenti di codice o righe estratte dal file TOML, né valori di configurazione
/// (testo di `SendText`, token o valori numerici inseriti dall'utente).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error("Errore di I/O durante l'accesso al file di configurazione: {0}")]
    Io(String),

    #[error("Errore di deserializzazione TOML: {message}")]
    TomlDe {
        message: String,
        line: Option<usize>,
        column: Option<usize>,
    },

    #[error("Errore di serializzazione TOML: {0}")]
    TomlSer(String),

    #[error("Versione schema TOML non supportata: attesa {expected}, trovata {found}")]
    UnsupportedSchemaVersion { expected: u32, found: u32 },

    #[error("ID regola duplicato rilevato nella configurazione: '{0}'")]
    DuplicateRuleId(String),

    #[error(
        "ID regola non valido (deve essere non vuoto e contenere solo caratteri alfanumerici, trattini o underscore)"
    )]
    InvalidRuleId,

    #[error(
        "Gruppo regola non valido (deve essere non vuoto e contenere solo caratteri ASCII alfanumerici, trattini o underscore)"
    )]
    InvalidRuleGroup,

    #[error("Una regola priva di gruppo non può essere abilitata")]
    EnabledRuleWithoutGroup,

    #[error("Il filtro applicazione richiede almeno un processo (.exe)")]
    EmptyWindowFilter,

    #[error(
        "Nome processo non valido nel filtro applicazione (indicare solo il nome eseguibile, senza percorsi)"
    )]
    InvalidProcessNameFilter,

    #[error("Il filtro applicazione supera la lunghezza massima consentita ({max} caratteri)")]
    WindowFilterTooLong { max: usize },

    #[error("Il nome della regola non può essere vuoto")]
    EmptyRuleName,

    #[error("Nome regola duplicato rilevato nella configurazione")]
    DuplicateRuleName,

    #[error("La combinazione di ripresa del daemon non può coincidere con il kill-switch")]
    ResumeHotkeyConflictsWithKillSwitch,

    #[error("La regola '{0}' ha una sequenza di azioni vuota")]
    EmptyActionSequence(String),

    #[error(
        "La regola '{rule_id}' supera il numero massimo consentito di azioni per sequenza ({count} > {max})"
    )]
    InvalidSequenceLength {
        rule_id: String,
        count: usize,
        max: usize,
    },

    #[error(
        "Il testo dell'azione SendText nella regola '{rule_id}' supera la lunghezza massima ({len} > {max} caratteri)"
    )]
    TextTooLong {
        rule_id: String,
        len: usize,
        max: usize,
    },

    #[error("Il testo dell'azione SendText nella regola '{0}' è vuoto")]
    EmptyText(String),

    #[error(
        "La regola '{rule_id}' specifica un ritardo fuori dai limiti consentiti (intervallo consentito: {min}..={max} ms)"
    )]
    InvalidDelay { rule_id: String, min: u64, max: u64 },

    #[error(
        "La regola '{rule_id}' specifica uno scorrimento mouse non valido (tacche consentite: ±1..=±{max})"
    )]
    InvalidMouseScroll { rule_id: String, max: i32 },

    #[error(
        "La regola '{rule_id}' specifica uno spostamento relativo mouse non valido (delta non nullo entro ±{max} px)"
    )]
    InvalidMouseMove { rule_id: String, max: i32 },

    #[error(
        "Valore di max_events_per_second fuori dai limiti consentiti (intervallo consentito: {min}..={max})"
    )]
    InvalidRateLimit { min: u32, max: u32 },

    #[error("Impossibile individuare la directory standard per i dati applicativi (%APPDATA%)")]
    AppDataNotFound,

    #[error("Percorso file non valido: {0}")]
    InvalidPath(PathBuf),

    #[error(
        "Rifiutata la sovrascrittura del file di configurazione esistente perché non valido (backup non autorizzato)"
    )]
    RefusedOverwriteInvalidFile,
}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        ConfigError::Io(err.to_string())
    }
}

impl ConfigError {
    /// Crea un `ConfigError::TomlDe` igienizzato a partire dall'errore TOML grezzo e dal contenuto del file.
    ///
    /// Preserva la categoria dell'errore (campo sconosciuto, errore di sintassi, campo mancante, tipo non valido)
    /// e le coordinate (riga e colonna), ma non espone MAI estratti del file sorgente né valori dei campi.
    pub fn from_toml_de_error(err: &toml::de::Error, content: &str) -> Self {
        let (line, column) = if let Some(span) = err.span() {
            let (l, c) = calculate_line_col(content, span.start);
            (Some(l), Some(c))
        } else {
            parse_line_col_from_str(&err.to_string())
        };

        let loc_str = match (line, column) {
            (Some(l), Some(c)) => format!(" alla riga {l} colonna {c}"),
            (Some(l), None) => format!(" alla riga {l}"),
            _ => String::new(),
        };

        let raw_msg = err.message();
        let message = if raw_msg.contains("unknown field") {
            format!("campo sconosciuto{loc_str}")
        } else if raw_msg.contains("missing field") {
            format!("campo obbligatorio mancante{loc_str}")
        } else if raw_msg.contains("invalid type") {
            format!("tipo di dato non valido{loc_str}")
        } else {
            format!("errore di sintassi{loc_str}")
        };

        ConfigError::TomlDe {
            message,
            line,
            column,
        }
    }
}

fn calculate_line_col(content: &str, span_start: usize) -> (usize, usize) {
    let clamped = span_start.min(content.len());
    let prefix = &content[..clamped];
    let line = prefix.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1;
    let last_newline = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    let column = clamped.saturating_sub(last_newline) + 1;
    (line, column)
}

fn parse_line_col_from_str(s: &str) -> (Option<usize>, Option<usize>) {
    if let Some(first_line) = s.lines().next() {
        if let Some(rest) = first_line.strip_prefix("TOML parse error at line ") {
            if let Some((line_part, col_part)) = rest.split_once(", column ") {
                let line = line_part.trim().parse::<usize>().ok();
                let col = col_part.trim().parse::<usize>().ok();
                return (line, col);
            }
        }
    }
    (None, None)
}
