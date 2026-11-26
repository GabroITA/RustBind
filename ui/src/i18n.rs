use crate::theme::{CustomTheme, ThemeId};
use rustbind::config::error::ConfigError;
use rustbind::core::action::InputAction;
use rustbind::engine::state::SkippedRuleReason;

/// Lingue supportate dalla UI di configurazione (predefinita: Inglese).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    Italian,
    French,
    German,
    Spanish,
}

impl Language {
    pub const ALL: &'static [Language] = &[
        Language::English,
        Language::Italian,
        Language::French,
        Language::German,
        Language::Spanish,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Italian => "it",
            Self::French => "fr",
            Self::German => "de",
            Self::Spanish => "es",
        }
    }

    pub fn from_code(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => Some(Self::English),
            "it" | "italiano" => Some(Self::Italian),
            "fr" | "français" | "francais" => Some(Self::French),
            "de" | "deutsch" => Some(Self::German),
            "es" | "español" | "espanol" => Some(Self::Spanish),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Italian => "Italiano",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
        }
    }

    pub fn s(self) -> &'static UiStrings {
        match self {
            Self::English => &EN,
            Self::Italian => &IT,
            Self::French => &FR,
            Self::German => &DE,
            Self::Spanish => &ES,
        }
    }

    pub fn theme_description(self, id: ThemeId) -> &'static str {
        match (self, id) {
            (Self::English, ThemeId::RustBindClassic) => "Dark graphite with copper accent #B7410E",
            (Self::English, ThemeId::GtaViViceSunset) => {
                "Leonida / Vice City vibe: night violet, neon pink and Miami cyan"
            }
            (Self::English, ThemeId::Dracula) => "High-contrast dark purple with lilac accents",
            (Self::English, ThemeId::NordNight) => "Easy-on-the-eyes Arctic slate blue palette",
            (Self::English, ThemeId::TokyoNight) => "Deep night indigo with bright blue highlights",
            (Self::English, ThemeId::CatppuccinLatte) => {
                "Soft pastel daylight tones with peach accent"
            }
            (Self::English, ThemeId::MacOsLight) => "Clean modern light desktop surfaces",
            (Self::English, ThemeId::RosePineDawn) => {
                "Warm dawn ivory with dusty rose and pine accents"
            }
            (Self::English, ThemeId::SolarizedLight) => {
                "Warm parchment palette with balanced contrast"
            }
            (Self::English, ThemeId::GruvboxLight) => "Warm retro paper with terracotta accent",
            (Self::English, ThemeId::Custom) => "User-defined custom color scheme and border radii",

            (Self::Italian, ThemeId::RustBindClassic) => "Grafite scura con accento rame #B7410E",
            (Self::Italian, ThemeId::GtaViViceSunset) => {
                "Ispirato a Leonida / Vice City: viola notte, rosa neon e ciano Miami"
            }
            (Self::Italian, ThemeId::Dracula) => "Viola scuro ad alto contrasto con accenti lilla",
            (Self::Italian, ThemeId::NordNight) => "Palette artica blu-ardesia morbida sugli occhi",
            (Self::Italian, ThemeId::TokyoNight) => {
                "Blu notte profondo con accenti indaco luminosi"
            }
            (Self::Italian, ThemeId::CatppuccinLatte) => {
                "Toni chiari pastello riposanti con accento pesca"
            }
            (Self::Italian, ThemeId::MacOsLight) => {
                "Superfici chiare e minimali stile desktop moderno"
            }
            (Self::Italian, ThemeId::RosePineDawn) => {
                "Avorio caldo rosato con accenti malva e pino"
            }
            (Self::Italian, ThemeId::SolarizedLight) => "Pergamena calda a contrasto bilanciato",
            (Self::Italian, ThemeId::GruvboxLight) => "Carta calda retrò con accento terracotta",
            (Self::Italian, ThemeId::Custom) => "Schema colori e raggi personalizzati dall'utente",

            (Self::French, ThemeId::RustBindClassic) => {
                "Graphite sombre avec accent cuivre #B7410E"
            }
            (Self::French, ThemeId::GtaViViceSunset) => {
                "Ambiance Vice City : violet nocturne, rose néon et cyan Miami"
            }
            (Self::French, ThemeId::Dracula) => {
                "Violet sombre à contraste élevé avec accents lilas"
            }
            (Self::French, ThemeId::NordNight) => "Palette arctique bleu ardoise reposante",
            (Self::French, ThemeId::TokyoNight) => {
                "Indigo nocturne profond avec touches bleu clair"
            }
            (Self::French, ThemeId::CatppuccinLatte) => {
                "Tons pastel clairs et doux avec accent pêche"
            }
            (Self::French, ThemeId::MacOsLight) => {
                "Surfaces claires épurées de style bureau moderne"
            }
            (Self::French, ThemeId::RosePineDawn) => {
                "Ivoire chaud poudré avec touches rose et sapin"
            }
            (Self::French, ThemeId::SolarizedLight) => "Parchemin chaud au contraste équilibré",
            (Self::French, ThemeId::GruvboxLight) => "Papier chaud rétro avec accent terre cuite",
            (Self::French, ThemeId::Custom) => "Thème personnalisé avec couleurs et rayons dédiés",

            (Self::German, ThemeId::RustBindClassic) => "Dunkles Graphit mit Kupfer-Akzent #B7410E",
            (Self::German, ThemeId::GtaViViceSunset) => {
                "Vice-City-Feeling: Nachtviolett, Neonpink und Miami-Cyan"
            }
            (Self::German, ThemeId::Dracula) => {
                "Kontrastreiches Dunkelviolett mit Flieder-Akzenten"
            }
            (Self::German, ThemeId::NordNight) => "Augenschonende arktische Schieferblau-Palette",
            (Self::German, ThemeId::TokyoNight) => "Tiefes Nachtindigo mit hellblauen Akzenten",
            (Self::German, ThemeId::CatppuccinLatte) => {
                "Sanfte helle Pastelltöne mit Pfirsich-Akzent"
            }
            (Self::German, ThemeId::MacOsLight) => "Klare, moderne helle Desktop-Oberflächen",
            (Self::German, ThemeId::RosePineDawn) => {
                "Warmes Rosé-Elfenbein mit Malve- und Pinienakzenten"
            }
            (Self::German, ThemeId::SolarizedLight) => "Warmes Pergament mit ausgewogenem Kontrast",
            (Self::German, ThemeId::GruvboxLight) => "Warmes Retro-Papier mit Terrakotta-Akzent",
            (Self::German, ThemeId::Custom) => "Benutzerdefinierte Farben und Eckenradien",

            (Self::Spanish, ThemeId::RustBindClassic) => "Grafito oscuro con acento cobre #B7410E",
            (Self::Spanish, ThemeId::GtaViViceSunset) => {
                "Estilo Vice City: violeta nocturno, rosa neón y cian Miami"
            }
            (Self::Spanish, ThemeId::Dracula) => "Morado oscuro de alto contraste con acentos lila",
            (Self::Spanish, ThemeId::NordNight) => {
                "Paleta ártica azul pizarra agradable a la vista"
            }
            (Self::Spanish, ThemeId::TokyoNight) => {
                "Índigo nocturno profundo con destellos azul claro"
            }
            (Self::Spanish, ThemeId::CatppuccinLatte) => {
                "Tonos claros pastel suaves con acento melocotón"
            }
            (Self::Spanish, ThemeId::MacOsLight) => {
                "Superficies claras limpias estilo escritorio moderno"
            }
            (Self::Spanish, ThemeId::RosePineDawn) => {
                "Marfil cálido rosado con acentos malva y pino"
            }
            (Self::Spanish, ThemeId::SolarizedLight) => {
                "Pergamino cálido con contraste equilibrado"
            }
            (Self::Spanish, ThemeId::GruvboxLight) => "Papel cálido retro con acento terracota",
            (Self::Spanish, ThemeId::Custom) => "Tema personalizado con colores y radios propios",
        }
    }

    pub fn fmt_custom_theme_details(self, custom: &CustomTheme) -> String {
        let r = custom.radii;
        let mode = match (self, custom.is_dark, custom.true_black) {
            (Self::English, true, true) => "Dark + True Black",
            (Self::English, true, false) => "Dark",
            (Self::English, false, _) => "Light",
            (Self::Italian, true, true) => "Scuro + True Black",
            (Self::Italian, true, false) => "Scuro",
            (Self::Italian, false, _) => "Chiaro",
            (Self::French, true, true) => "Sombre + True Black",
            (Self::French, true, false) => "Sombre",
            (Self::French, false, _) => "Clair",
            (Self::German, true, true) => "Dunkel + True Black",
            (Self::German, true, false) => "Dunkel",
            (Self::German, false, _) => "Hell",
            (Self::Spanish, true, true) => "Oscuro + True Black",
            (Self::Spanish, true, false) => "Oscuro",
            (Self::Spanish, false, _) => "Claro",
        };
        match self {
            Self::English => format!(
                "Mode: {mode} | Radii: controls {}px, boxes {}px, cards {}px, modals {}px",
                r.controls, r.inner_boxes, r.cards_menus, r.modals
            ),
            Self::Italian => format!(
                "Modalità: {mode} | Raggi: controlli {}px, box {}px, card {}px, modali {}px",
                r.controls, r.inner_boxes, r.cards_menus, r.modals
            ),
            Self::French => format!(
                "Mode : {mode} | Rayons : contrôles {}px, cadres {}px, cartes {}px, modales {}px",
                r.controls, r.inner_boxes, r.cards_menus, r.modals
            ),
            Self::German => format!(
                "Modus: {mode} | Radien: Steuerelemente {}px, Felder {}px, Karten {}px, Dialoge {}px",
                r.controls, r.inner_boxes, r.cards_menus, r.modals
            ),
            Self::Spanish => format!(
                "Modo: {mode} | Radios: controles {}px, cajas {}px, tarjetas {}px, modales {}px",
                r.controls, r.inner_boxes, r.cards_menus, r.modals
            ),
        }
    }

    pub fn tr_config_error(self, err: &ConfigError) -> String {
        match (self, err) {
            (Self::English, ConfigError::UnsupportedSchemaVersion { expected, found }) => {
                format!("Unsupported TOML schema version: expected {expected}, found {found}")
            }
            (Self::English, ConfigError::DuplicateRuleId(_)) => {
                "Duplicate rule ID detected in configuration".to_string()
            }
            (Self::English, ConfigError::InvalidRuleId) => {
                "Invalid rule ID (ASCII letters, digits, '_' or '-')".to_string()
            }
            (Self::English, ConfigError::InvalidRuleGroup) => {
                "Invalid rule group (ASCII letters, digits, '_' or '-')".to_string()
            }
            (Self::English, ConfigError::EnabledRuleWithoutGroup) => {
                "A rule without a group cannot be enabled".to_string()
            }
            (Self::English, ConfigError::EmptyWindowFilter) => {
                "Application filter requires at least one process (.exe)".to_string()
            }
            (Self::English, ConfigError::InvalidProcessNameFilter) => {
                "Invalid process name in application filter (filename only, no path)".to_string()
            }
            (Self::English, ConfigError::WindowFilterTooLong { max }) => {
                format!("Application filter exceeds max length ({max} chars)")
            }
            (Self::English, ConfigError::EmptyRuleName) => "Rule name cannot be empty".to_string(),
            (Self::English, ConfigError::DuplicateRuleName) => {
                "Duplicate rule name detected in configuration".to_string()
            }
            (Self::English, ConfigError::ResumeHotkeyConflictsWithKillSwitch) => {
                "Resume hotkey cannot match the kill-switch".to_string()
            }
            (Self::English, ConfigError::EmptyActionSequence(_)) => {
                "A rule has an empty action sequence".to_string()
            }
            (Self::English, ConfigError::InvalidSequenceLength { count, max, .. }) => {
                format!("A rule exceeds the maximum action count ({count} > {max})")
            }
            (Self::English, ConfigError::TextTooLong { len, max, .. }) => {
                format!("SendText action exceeds max length ({len} > {max} chars)")
            }
            (Self::English, ConfigError::EmptyText(_)) => {
                "SendText action text cannot be empty".to_string()
            }
            (Self::English, ConfigError::InvalidDelay { min, max, .. }) => {
                format!("Delay value out of range ({min}..={max} ms)")
            }
            (Self::English, ConfigError::InvalidMouseScroll { max, .. }) => {
                format!("Mouse scroll steps out of range (±1..=±{max})")
            }
            (Self::English, ConfigError::InvalidMouseMove { max, .. }) => {
                format!("Mouse move delta invalid (non-zero within ±{max} px)")
            }
            (Self::English, ConfigError::InvalidRateLimit { min, max }) => {
                format!("max_events_per_second out of range ({min}..={max})")
            }
            (Self::English, ConfigError::AppDataNotFound) => {
                "Unable to locate %APPDATA% directory".to_string()
            }
            (Self::English, ConfigError::RefusedOverwriteInvalidFile) => {
                "Refused overwriting invalid config file without backup".to_string()
            }
            (Self::English, ConfigError::TomlDe { line, column, .. }) => match (line, column) {
                (Some(l), Some(c)) => format!("Invalid TOML configuration at line {l}, column {c}"),
                (Some(l), None) => format!("Invalid TOML configuration at line {l}"),
                _ => "Invalid TOML configuration".to_string(),
            },
            (Self::English, other) => other.to_string(),

            (Self::Italian, other) => other.to_string(),

            (Self::French, ConfigError::UnsupportedSchemaVersion { expected, found }) => {
                format!(
                    "Version du schéma TOML non prise en charge : attendu {expected}, trouvé {found}"
                )
            }
            (Self::French, ConfigError::DuplicateRuleId(_)) => {
                "Identifiant de règle en double détecté".to_string()
            }
            (Self::French, ConfigError::InvalidRuleId) => {
                "Identifiant de règle invalide (alphanumérique ASCII, '_' ou '-')".to_string()
            }
            (Self::French, ConfigError::InvalidRuleGroup) => {
                "Groupe de règle invalide (alphanumérique ASCII, '_' ou '-')".to_string()
            }
            (Self::French, ConfigError::EnabledRuleWithoutGroup) => {
                "Une règle sans groupe ne peut pas être activée".to_string()
            }
            (Self::French, ConfigError::EmptyWindowFilter) => {
                "Le filtre d'application requiert au moins un processus (.exe)".to_string()
            }
            (Self::French, ConfigError::InvalidProcessNameFilter) => {
                "Nom de processus invalide dans le filtre (nom du fichier seul, sans chemin)"
                    .to_string()
            }
            (Self::French, ConfigError::WindowFilterTooLong { max }) => {
                format!("Le filtre d'application dépasse la longueur maximale ({max} caractères)")
            }
            (Self::French, ConfigError::EmptyRuleName) => {
                "Le nom de la règle ne peut pas être vide".to_string()
            }
            (Self::French, ConfigError::DuplicateRuleName) => {
                "Nom de règle en double détecté".to_string()
            }
            (Self::French, ConfigError::ResumeHotkeyConflictsWithKillSwitch) => {
                "Le raccourci de reprise ne peut pas être identique au kill-switch".to_string()
            }
            (Self::French, ConfigError::EmptyActionSequence(_)) => {
                "Une règle possède une séquence d'actions vide".to_string()
            }
            (Self::French, ConfigError::InvalidSequenceLength { count, max, .. }) => {
                format!("Une règle dépasse la limite d'actions ({count} > {max})")
            }
            (Self::French, ConfigError::TextTooLong { len, max, .. }) => {
                format!("Le texte SendText dépasse la limite ({len} > {max} caractères)")
            }
            (Self::French, ConfigError::EmptyText(_)) => {
                "Le texte d'une action SendText ne peut pas être vide".to_string()
            }
            (Self::French, ConfigError::InvalidDelay { min, max, .. }) => {
                format!("Délai hors limites ({min}..={max} ms)")
            }
            (Self::French, ConfigError::InvalidMouseScroll { max, .. }) => {
                format!("Défilement souris hors limites (±1..=±{max})")
            }
            (Self::French, ConfigError::InvalidMouseMove { max, .. }) => {
                format!("Déplacement souris invalide (non nul dans ±{max} px)")
            }
            (Self::French, ConfigError::InvalidRateLimit { min, max }) => {
                format!("max_events_per_second hors limites ({min}..={max})")
            }
            (Self::French, ConfigError::TomlDe { line, column, .. }) => match (line, column) {
                (Some(l), Some(c)) => format!("Configuration TOML invalide ligne {l}, colonne {c}"),
                (Some(l), None) => format!("Configuration TOML invalide ligne {l}"),
                _ => "Configuration TOML invalide".to_string(),
            },
            (Self::French, other) => other.to_string(),

            (Self::German, ConfigError::UnsupportedSchemaVersion { expected, found }) => {
                format!(
                    "Nicht unterstützte TOML-Schemaversion: erwartet {expected}, gefunden {found}"
                )
            }
            (Self::German, ConfigError::DuplicateRuleId(_)) => {
                "Doppelte Regel-ID in der Konfiguration erkannt".to_string()
            }
            (Self::German, ConfigError::InvalidRuleId) => {
                "Ungültige Regel-ID (ASCII-Buchstaben, Ziffern, '_' oder '-')".to_string()
            }
            (Self::German, ConfigError::InvalidRuleGroup) => {
                "Ungültige Regelgruppe (ASCII-Buchstaben, Ziffern, '_' oder '-')".to_string()
            }
            (Self::German, ConfigError::EnabledRuleWithoutGroup) => {
                "Eine Regel ohne Gruppe kann nicht aktiviert werden".to_string()
            }
            (Self::German, ConfigError::EmptyWindowFilter) => {
                "Anwendungsfilter erfordert mindestens einen Prozess (.exe)".to_string()
            }
            (Self::German, ConfigError::InvalidProcessNameFilter) => {
                "Ungültiger Prozessname im Anwendungsfilter (nur Dateiname ohne Pfad)".to_string()
            }
            (Self::German, ConfigError::WindowFilterTooLong { max }) => {
                format!("Anwendungsfilter überschreitet die Maximallänge ({max} Zeichen)")
            }
            (Self::German, ConfigError::EmptyRuleName) => {
                "Der Regelname darf nicht leer sein".to_string()
            }
            (Self::German, ConfigError::DuplicateRuleName) => {
                "Doppelter Regelname in der Konfiguration erkannt".to_string()
            }
            (Self::German, ConfigError::ResumeHotkeyConflictsWithKillSwitch) => {
                "Der Fortsetzen-Hotkey darf nicht dem Kill-Switch entsprechen".to_string()
            }
            (Self::German, ConfigError::EmptyActionSequence(_)) => {
                "Eine Regel enthält eine leere Aktionsfolge".to_string()
            }
            (Self::German, ConfigError::InvalidSequenceLength { count, max, .. }) => {
                format!("Eine Regel überschreitet das Aktionslimit ({count} > {max})")
            }
            (Self::German, ConfigError::TextTooLong { len, max, .. }) => {
                format!("SendText überschreitet die Maximallänge ({len} > {max} Zeichen)")
            }
            (Self::German, ConfigError::EmptyText(_)) => {
                "SendText-Text darf nicht leer sein".to_string()
            }
            (Self::German, ConfigError::InvalidDelay { min, max, .. }) => {
                format!("Verzögerung außerhalb des Bereichs ({min}..={max} ms)")
            }
            (Self::German, ConfigError::InvalidMouseScroll { max, .. }) => {
                format!("Mausrad-Schritte außerhalb des Bereichs (±1..=±{max})")
            }
            (Self::German, ConfigError::InvalidMouseMove { max, .. }) => {
                format!("Mausbewegung ungültig (ungleich null innerhalb ±{max} px)")
            }
            (Self::German, ConfigError::InvalidRateLimit { min, max }) => {
                format!("max_events_per_second außerhalb des Bereichs ({min}..={max})")
            }
            (Self::German, ConfigError::TomlDe { line, column, .. }) => match (line, column) {
                (Some(l), Some(c)) => {
                    format!("Ungültige TOML-Konfiguration in Zeile {l}, Spalte {c}")
                }
                (Some(l), None) => format!("Ungültige TOML-Konfiguration in Zeile {l}"),
                _ => "Ungültige TOML-Konfiguration".to_string(),
            },
            (Self::German, other) => other.to_string(),

            (Self::Spanish, ConfigError::UnsupportedSchemaVersion { expected, found }) => {
                format!(
                    "Versión de esquema TOML no compatible: se esperaba {expected}, se encontró {found}"
                )
            }
            (Self::Spanish, ConfigError::DuplicateRuleId(_)) => {
                "ID de regla duplicado detectado en la configuración".to_string()
            }
            (Self::Spanish, ConfigError::InvalidRuleId) => {
                "ID de regla no válido (alfanumérico ASCII, '_' o '-')".to_string()
            }
            (Self::Spanish, ConfigError::InvalidRuleGroup) => {
                "Grupo de regla no válido (alfanumérico ASCII, '_' o '-')".to_string()
            }
            (Self::Spanish, ConfigError::EnabledRuleWithoutGroup) => {
                "No se puede activar una regla sin grupo".to_string()
            }
            (Self::Spanish, ConfigError::EmptyWindowFilter) => {
                "El filtro de aplicación requiere al menos un proceso (.exe)".to_string()
            }
            (Self::Spanish, ConfigError::InvalidProcessNameFilter) => {
                "Nombre de proceso no válido en el filtro (solo nombre de archivo, sin ruta)"
                    .to_string()
            }
            (Self::Spanish, ConfigError::WindowFilterTooLong { max }) => {
                format!("El filtro de aplicación supera el límite ({max} caracteres)")
            }
            (Self::Spanish, ConfigError::EmptyRuleName) => {
                "El nombre de la regla no puede estar vacío".to_string()
            }
            (Self::Spanish, ConfigError::DuplicateRuleName) => {
                "Nombre de regla duplicado detectado en la configuración".to_string()
            }
            (Self::Spanish, ConfigError::ResumeHotkeyConflictsWithKillSwitch) => {
                "El atajo para reanudar no puede coincidir con el kill-switch".to_string()
            }
            (Self::Spanish, ConfigError::EmptyActionSequence(_)) => {
                "Una regla tiene una secuencia de acciones vacía".to_string()
            }
            (Self::Spanish, ConfigError::InvalidSequenceLength { count, max, .. }) => {
                format!("Una regla supera el límite de acciones ({count} > {max})")
            }
            (Self::Spanish, ConfigError::TextTooLong { len, max, .. }) => {
                format!("SendText supera la longitud máxima ({len} > {max} caracteres)")
            }
            (Self::Spanish, ConfigError::EmptyText(_)) => {
                "El texto de SendText no puede estar vacío".to_string()
            }
            (Self::Spanish, ConfigError::InvalidDelay { min, max, .. }) => {
                format!("Retraso fuera de rango ({min}..={max} ms)")
            }
            (Self::Spanish, ConfigError::InvalidMouseScroll { max, .. }) => {
                format!("Pasos de rueda fuera de rango (±1..=±{max})")
            }
            (Self::Spanish, ConfigError::InvalidMouseMove { max, .. }) => {
                format!("Desplazamiento del ratón no válido (no nulo dentro de ±{max} px)")
            }
            (Self::Spanish, ConfigError::InvalidRateLimit { min, max }) => {
                format!("max_events_per_second fuera de rango ({min}..={max})")
            }
            (Self::Spanish, ConfigError::TomlDe { line, column, .. }) => match (line, column) {
                (Some(l), Some(c)) => {
                    format!("Configuración TOML no válida en línea {l}, columna {c}")
                }
                (Some(l), None) => format!("Configuración TOML no válida en línea {l}"),
                _ => "Configuración TOML no válida".to_string(),
            },
            (Self::Spanish, other) => other.to_string(),
        }
    }

    pub fn fmt_rules_count(self, n: usize) -> String {
        match self {
            Self::English => format!("Rules ({n})"),
            Self::Italian => format!("Regole ({n})"),
            Self::French => format!("Règles ({n})"),
            Self::German => format!("Regeln ({n})"),
            Self::Spanish => format!("Reglas ({n})"),
        }
    }

    pub fn fmt_context_badge(self, exclude: bool, summary: &str) -> String {
        match (self, exclude) {
            (Self::English, false) => format!("Only: {summary}"),
            (Self::English, true) => format!("Except: {summary}"),
            (Self::Italian, false) => format!("Solo: {summary}"),
            (Self::Italian, true) => format!("Tranne: {summary}"),
            (Self::French, false) => format!("Uniquement : {summary}"),
            (Self::French, true) => format!("Sauf : {summary}"),
            (Self::German, false) => format!("Nur: {summary}"),
            (Self::German, true) => format!("Außer: {summary}"),
            (Self::Spanish, false) => format!("Solo: {summary}"),
            (Self::Spanish, true) => format!("Excepto: {summary}"),
        }
    }

    pub fn tr_skipped_reason(self, reason: SkippedRuleReason) -> &'static str {
        match (self, reason) {
            (Self::English, SkippedRuleReason::OsAlreadyRegistered) => {
                "Hotkey already claimed in Windows by another application"
            }
            (Self::English, SkippedRuleReason::OsRegistrationFailed) => {
                "Windows refused registering this hotkey (OS error)"
            }
            (Self::English, SkippedRuleReason::ConflictsWithSystemHotkey) => {
                "Conflicts with Kill-Switch or Resume hotkey"
            }
            (Self::English, SkippedRuleReason::ShadowedByEarlierRule) => {
                "Shadowed by an earlier active rule with the same trigger"
            }

            (Self::Italian, SkippedRuleReason::OsAlreadyRegistered) => {
                "Scorciatoia già occupata in Windows da un'altra applicazione"
            }
            (Self::Italian, SkippedRuleReason::OsRegistrationFailed) => {
                "Windows ha rifiutato la registrazione di questa scorciatoia"
            }
            (Self::Italian, SkippedRuleReason::ConflictsWithSystemHotkey) => {
                "Coincide con il Kill-Switch o con l'hotkey di ripresa"
            }
            (Self::Italian, SkippedRuleReason::ShadowedByEarlierRule) => {
                "Oscurata da una regola attiva precedente con lo stesso trigger"
            }

            (Self::French, SkippedRuleReason::OsAlreadyRegistered) => {
                "Raccourci déjà utilisé sous Windows par une autre application"
            }
            (Self::French, SkippedRuleReason::OsRegistrationFailed) => {
                "Windows a refusé l'enregistrement de ce raccourci"
            }
            (Self::French, SkippedRuleReason::ConflictsWithSystemHotkey) => {
                "Entre en conflit avec le Kill-Switch ou le raccourci de reprise"
            }
            (Self::French, SkippedRuleReason::ShadowedByEarlierRule) => {
                "Masquée par une règle active précédente avec le même déclencheur"
            }

            (Self::German, SkippedRuleReason::OsAlreadyRegistered) => {
                "Tastenkombination wird in Windows bereits von einer anderen App belegt"
            }
            (Self::German, SkippedRuleReason::OsRegistrationFailed) => {
                "Windows hat die Registrierung dieses Hotkeys abgelehnt"
            }
            (Self::German, SkippedRuleReason::ConflictsWithSystemHotkey) => {
                "Konflikt mit Kill-Switch oder Fortsetzen-Hotkey"
            }
            (Self::German, SkippedRuleReason::ShadowedByEarlierRule) => {
                "Verdeckt durch eine frühere aktive Regel mit demselben Auslöser"
            }

            (Self::Spanish, SkippedRuleReason::OsAlreadyRegistered) => {
                "Atajo ya ocupado en Windows por otra aplicación"
            }
            (Self::Spanish, SkippedRuleReason::OsRegistrationFailed) => {
                "Windows rechazó el registro de este atajo"
            }
            (Self::Spanish, SkippedRuleReason::ConflictsWithSystemHotkey) => {
                "Entra en conflicto con el Kill-Switch o el atajo de reanudación"
            }
            (Self::Spanish, SkippedRuleReason::ShadowedByEarlierRule) => {
                "Ocultada por una regla activa anterior con el mismo disparador"
            }
        }
    }

    pub fn fmt_conflict_duplicate_shadowed(self, first_names: &str) -> String {
        match self {
            Self::English => format!(
                "Duplicate trigger with earlier active rule ({first_names}): this rule will be skipped."
            ),
            Self::Italian => format!(
                "Trigger duplicato con una regola precedente ({first_names}): questa regola verrà saltata."
            ),
            Self::French => format!(
                "Déclencheur en double avec une règle précédente ({first_names}) : cette règle sera ignorée."
            ),
            Self::German => format!(
                "Doppelter Auslöser mit früherer Regel ({first_names}): Diese Regel wird übersprungen."
            ),
            Self::Spanish => format!(
                "Disparador duplicado con una regla anterior ({first_names}): esta regla se omitirá."
            ),
        }
    }

    pub fn fmt_conflict_duplicate_ahead(self, later_names: &str) -> String {
        match self {
            Self::English => {
                format!("Shares trigger with later rule ({later_names}), which will be skipped.")
            }
            Self::Italian => {
                format!(
                    "Condivide il trigger con la regola successiva ({later_names}), che verrà saltata."
                )
            }
            Self::French => format!(
                "Partage le déclencheur avec une règle suivante ({later_names}), qui sera ignorée."
            ),
            Self::German => format!(
                "Teilt den Auslöser mit einer späteren Regel ({later_names}), die übersprungen wird."
            ),
            Self::Spanish => format!(
                "Comparte el disparador con una regla posterior ({later_names}), que se omitirá."
            ),
        }
    }

    pub fn fmt_settings_hotkey_conflict(self, rule_names: &str) -> String {
        match self {
            Self::English => format!("Conflicts with rule(s): {rule_names}"),
            Self::Italian => format!("In conflitto con la/le regola/e: {rule_names}"),
            Self::French => format!("En conflit avec la/les règle(s) : {rule_names}"),
            Self::German => format!("Konflikt mit Regel(n): {rule_names}"),
            Self::Spanish => format!("En conflicto con la(s) regla(s): {rule_names}"),
        }
    }

    pub fn fmt_actions_header(self, count: usize, max: usize) -> String {
        match self {
            Self::English => format!("Actions ({count} / {max})"),
            Self::Italian => format!("Azioni ({count} / {max})"),
            Self::French => format!("Actions ({count} / {max})"),
            Self::German => format!("Aktionen ({count} / {max})"),
            Self::Spanish => format!("Acciones ({count} / {max})"),
        }
    }

    pub fn fmt_rules_in_group_title(self, group: &str) -> String {
        match self {
            Self::English => format!("Rules in group \"{group}\""),
            Self::Italian => format!("Regole nel gruppo \"{group}\""),
            Self::French => format!("Règles dans le groupe « {group} »"),
            Self::German => format!("Regeln in Gruppe „{group}“"),
            Self::Spanish => format!("Reglas en el grupo \"{group}\""),
        }
    }

    pub fn fmt_duplicate_cluster_title(self, idx: usize, rules: usize, actions: usize) -> String {
        match self {
            Self::English => {
                format!("Duplicate rules #{idx} — {rules} rules ({actions} identical actions)")
            }
            Self::Italian => {
                format!("Regole duplicate #{idx} — {rules} regole ({actions} azioni identiche)")
            }
            Self::French => {
                format!("Règles en double #{idx} — {rules} règles ({actions} actions identiques)")
            }
            Self::German => {
                format!("Doppelte Regeln #{idx} — {rules} Regeln ({actions} identische Aktionen)")
            }
            Self::Spanish => {
                format!("Reglas duplicadas #{idx} — {rules} reglas ({actions} acciones idénticas)")
            }
        }
    }

    pub fn fmt_resolve_duplicates_btn(self, remove_count: usize) -> String {
        match self {
            Self::English => format!(
                "Keep selected & delete {remove_count} duplicate{}",
                if remove_count == 1 { "" } else { "s" }
            ),
            Self::Italian => format!(
                "Mantieni selezionata ed elimina {remove_count} duplicat{}",
                if remove_count == 1 { "o" } else { "i" }
            ),
            Self::French => format!(
                "Conserver et supprimer {remove_count} doublon{}",
                if remove_count == 1 { "" } else { "s" }
            ),
            Self::German => format!(
                "Ausgewählte behalten & {remove_count} Duplikat{} löschen",
                if remove_count == 1 { "" } else { "e" }
            ),
            Self::Spanish => format!(
                "Conservar y eliminar {remove_count} duplicad{}",
                if remove_count == 1 { "o" } else { "os" }
            ),
        }
    }

    pub fn fmt_delete_rule_confirm(self, name: &str) -> String {
        match self {
            Self::English => format!(
                "Permanently delete rule \"{name}\"?\nIt will also be removed from all groups."
            ),
            Self::Italian => format!(
                "Eliminare definitivamente la regola \"{name}\"?\nVerrà rimossa anche da tutti i gruppi che la contengono."
            ),
            Self::French => format!(
                "Supprimer définitivement la règle « {name} » ?\nElle sera aussi retirée de tous les groupes."
            ),
            Self::German => format!(
                "Regel „{name}“ dauerhaft löschen?\nSie wird auch aus allen Gruppen entfernt."
            ),
            Self::Spanish => format!(
                "¿Eliminar definitivamente la regla \"{name}\"?\nTambién se quitará de todos los grupos."
            ),
        }
    }

    pub fn fmt_delete_group_confirm(self, name: &str) -> String {
        match self {
            Self::English => format!(
                "Delete group \"{name}\"?\nRules that belonged only to this group will remain groupless and disabled."
            ),
            Self::Italian => format!(
                "Eliminare il gruppo \"{name}\"?\nLe regole presenti solo in questo gruppo resteranno senza gruppo e saranno disattivate."
            ),
            Self::French => format!(
                "Supprimer le groupe « {name} » ?\nLes règles présentes uniquement dans ce groupe resteront sans groupe et désactivées."
            ),
            Self::German => format!(
                "Gruppe „{name}“ löschen?\nRegeln nur in dieser Gruppe bleiben gruppenlos und deaktiviert."
            ),
            Self::Spanish => format!(
                "¿Eliminar el grupo \"{name}\"?\nLas reglas que solo estaban en este grupo quedarán sin grupo y desactivadas."
            ),
        }
    }

    pub fn fmt_resolve_duplicates_confirm(
        self,
        keep_name: &str,
        count: usize,
        merge_groups: bool,
    ) -> String {
        match self {
            Self::English => {
                let note = if merge_groups {
                    "\nGroups from deleted rules will be assigned to the kept rule."
                } else {
                    ""
                };
                format!("Keep rule \"{keep_name}\" and delete {count} duplicate rule(s)?{note}")
            }
            Self::Italian => {
                let note = if merge_groups {
                    "\nI gruppi delle regole eliminate verranno assegnati alla regola mantenuta."
                } else {
                    ""
                };
                format!(
                    "Mantenere la regola \"{keep_name}\" ed eliminare {count} regol{} duplicat{}?{note}",
                    if count == 1 { "a" } else { "e" },
                    if count == 1 { "a" } else { "e" },
                )
            }
            Self::French => {
                let note = if merge_groups {
                    "\nLes groupes des règles supprimées seront ajoutés à la règle conservée."
                } else {
                    ""
                };
                format!("Conserver « {keep_name} » et supprimer {count} règle(s) en double ?{note}")
            }
            Self::German => {
                let note = if merge_groups {
                    "\nGruppen der gelöschten Regeln werden der behaltenen Regel zugewiesen."
                } else {
                    ""
                };
                format!("Regel „{keep_name}“ behalten und {count} doppelte Regel(n) löschen?{note}")
            }
            Self::Spanish => {
                let note = if merge_groups {
                    "\nLos grupos de las reglas eliminadas se asignarán a la regla conservada."
                } else {
                    ""
                };
                format!(
                    "¿Conservar \"{keep_name}\" y eliminar {count} regla(s) duplicada(s)?{note}"
                )
            }
        }
    }

    pub fn fmt_import_overwrite_confirm(self, target_name: &str) -> String {
        match self {
            Self::English => format!(
                "A configuration file named \"{target_name}\" already exists.\nContinuing will overwrite it with the imported file. Continue?"
            ),
            Self::Italian => format!(
                "Esiste già un file di configurazione chiamato \"{target_name}\".\nProcedendo verrà sovrascritto con il file importato. Continuare?"
            ),
            Self::French => format!(
                "Un fichier de configuration nommé « {target_name} » existe déjà.\nContinuer l'écrasera avec le fichier importé. Continuer ?"
            ),
            Self::German => format!(
                "Eine Konfigurationsdatei namens „{target_name}“ existiert bereits.\nFortfahren überschreibt sie mit der importierten Datei. Weiter?"
            ),
            Self::Spanish => format!(
                "Ya existe un archivo de configuración llamado \"{target_name}\".\nSi continúas, se sobrescribirá con el archivo importado. ¿Continuar?"
            ),
        }
    }

    pub fn fmt_switch_profile_dirty_confirm(self, target_name: &str) -> String {
        match self {
            Self::English => format!(
                "Switching to configuration \"{target_name}\" will discard your current unsaved changes.\nContinue?"
            ),
            Self::Italian => format!(
                "Passando alla configurazione \"{target_name}\" le modifiche non salvate correnti andranno perse.\nContinuare?"
            ),
            Self::French => format!(
                "Passer à « {target_name} » annulera vos modifications non enregistrées.\nContinuer ?"
            ),
            Self::German => format!(
                "Beim Wechsel zu „{target_name}“ gehen ungespeicherte Änderungen verloren.\nFortfahren?"
            ),
            Self::Spanish => format!(
                "Al cambiar a \"{target_name}\" se perderán los cambios no guardados.\n¿Continuar?"
            ),
        }
    }

    pub fn fmt_ipc_loaded_hotkeys(self, n: u32) -> String {
        match self {
            Self::English => format!("Daemon loaded hotkeys (IPC): {n}"),
            Self::Italian => format!("Hotkey caricate nel daemon (IPC): {n}"),
            Self::French => format!("Raccourcis chargés dans le démon (IPC) : {n}"),
            Self::German => format!("Im Daemon geladene Hotkeys (IPC): {n}"),
            Self::Spanish => format!("Atajos cargados en el daemon (IPC): {n}"),
        }
    }

    pub fn fmt_group_inherited_context(self, summary: &str) -> String {
        match self {
            Self::English => format!("Inherited from active groups: {summary}"),
            Self::Italian => format!("Ereditato dai gruppi attivi: {summary}"),
            Self::French => format!("Hérité des groupes actifs : {summary}"),
            Self::German => format!("Von aktiven Gruppen geerbt: {summary}"),
            Self::Spanish => format!("Heredado de grupos activos: {summary}"),
        }
    }

    pub fn fmt_group_rules_added(self, count: usize, group: &str) -> String {
        match self {
            Self::English => format!("{count} rule(s) added to group '{group}'."),
            Self::Italian => format!("{count} regole aggiunte al gruppo '{group}'."),
            Self::French => format!("{count} règle(s) ajoutée(s) au groupe « {group} »."),
            Self::German => format!("{count} Regel(n) zu Gruppe „{group}“ hinzugefügt."),
            Self::Spanish => format!("{count} regla(s) añadida(s) al grupo '{group}'."),
        }
    }

    pub fn fmt_group_rules_removed(self, count: usize, group: &str) -> String {
        match self {
            Self::English => format!("{count} rule(s) removed from group '{group}'."),
            Self::Italian => format!("{count} regole rimosse dal gruppo '{group}'."),
            Self::French => format!("{count} règle(s) retirée(s) du groupe « {group} »."),
            Self::German => format!("{count} Regel(n) aus Gruppe „{group}“ entfernt."),
            Self::Spanish => format!("{count} regla(s) quitada(s) del grupo '{group}'."),
        }
    }

    pub fn fmt_rule_deleted(self, name: &str) -> String {
        match self {
            Self::English => format!("Rule '{name}' deleted and removed from groups."),
            Self::Italian => format!("Regola '{name}' eliminata e rimossa dai gruppi."),
            Self::French => format!("Règle « {name} » supprimée et retirée des groupes."),
            Self::German => format!("Regel „{name}“ gelöscht und aus Gruppen entfernt."),
            Self::Spanish => format!("Regla '{name}' eliminada y quitada de los grupos."),
        }
    }

    pub fn fmt_group_deleted(self, name: &str) -> String {
        match self {
            Self::English => format!("Group '{name}' deleted."),
            Self::Italian => format!("Gruppo '{name}' eliminato."),
            Self::French => format!("Groupe « {name} » supprimé."),
            Self::German => format!("Gruppe „{name}“ gelöscht."),
            Self::Spanish => format!("Grupo '{name}' eliminado."),
        }
    }

    pub fn fmt_duplicates_removed(self, count: usize) -> String {
        match self {
            Self::English => format!("Removed {count} duplicate rule(s), keeping selected rule."),
            Self::Italian => {
                format!("Rimosse {count} regole duplicate mantenendo la regola selezionata.")
            }
            Self::French => {
                format!("{count} règle(s) en double supprimée(s) ; règle sélectionnée conservée.")
            }
            Self::German => format!("{count} doppelte Regel(n) gelöscht, ausgewählte behalten."),
            Self::Spanish => {
                format!("Se eliminaron {count} regla(s) duplicada(s), conservando la seleccionada.")
            }
        }
    }

    pub fn fmt_shared_actions_title(self, count: usize) -> String {
        match self {
            Self::English => format!("Shared actions ({count})"),
            Self::Italian => format!("Azioni condivise ({count})"),
            Self::French => format!("Actions partagées ({count})"),
            Self::German => format!("Gemeinsame Aktionen ({count})"),
            Self::Spanish => format!("Acciones compartidas ({count})"),
        }
    }

    pub fn format_action_preview(self, idx: usize, action: &InputAction) -> String {
        match action {
            InputAction::KeyClick { key } => format!("{}. KeyClick: {}", idx + 1, key.as_str()),
            InputAction::KeyDown { key } => format!("{}. KeyDown: {}", idx + 1, key.as_str()),
            InputAction::KeyUp { key } => format!("{}. KeyUp: {}", idx + 1, key.as_str()),
            InputAction::Delay { milliseconds } => {
                format!("{}. Delay: {} ms", idx + 1, milliseconds)
            }
            InputAction::SendText { text } => {
                let preview: String = text.chars().take(36).collect();
                let suffix = if text.chars().count() > 36 { "…" } else { "" };
                let label = match self {
                    Self::English => "chars",
                    Self::Italian => "car.",
                    Self::French => "car.",
                    Self::German => "Zeichen",
                    Self::Spanish => "car.",
                };
                format!(
                    "{}. SendText ({} {}): \"{}{}\"",
                    idx + 1,
                    text.chars().count(),
                    label,
                    preview,
                    suffix
                )
            }
            InputAction::MouseClick { button } => {
                format!("{}. MouseClick: {}", idx + 1, button.as_str())
            }
            InputAction::MouseDown { button } => {
                format!("{}. MouseDown: {}", idx + 1, button.as_str())
            }
            InputAction::MouseUp { button } => {
                format!("{}. MouseUp: {}", idx + 1, button.as_str())
            }
            InputAction::MouseScroll { direction, steps } => {
                format!(
                    "{}. MouseScroll: {} ({})",
                    idx + 1,
                    direction.as_str(),
                    steps
                )
            }
            InputAction::MouseMove { dx, dy } => {
                format!("{}. MouseMove: dx={}, dy={}", idx + 1, dx, dy)
            }
        }
    }

    pub fn fmt_default_rule_name(self, raw_num: &str) -> String {
        match self {
            Self::English => format!("Rule {raw_num}"),
            Self::Italian => format!("Regola {raw_num}"),
            Self::French => format!("Règle {raw_num}"),
            Self::German => format!("Regel {raw_num}"),
            Self::Spanish => format!("Regla {raw_num}"),
        }
    }

    pub fn fmt_duplicate_rule_name(self, source_name: &str) -> String {
        let suffix = match self {
            Self::English => "Copy",
            Self::Italian => "Copia",
            Self::French => "Copie",
            Self::German => "Kopie",
            Self::Spanish => "Copia",
        };
        format!("{source_name} ({suffix})")
    }

    pub fn fmt_imported_rule_actions(self, count: usize, source_name: &str) -> String {
        match self {
            Self::English => format!("Imported {count} actions from '{source_name}'."),
            Self::Italian => format!("Importate {count} azioni da '{source_name}'."),
            Self::French => format!("{count} actions importées depuis « {source_name} »."),
            Self::German => format!("{count} Aktionen aus „{source_name}“ importiert."),
            Self::Spanish => format!("Se importaron {count} acciones desde '{source_name}'."),
        }
    }

    pub fn fmt_update_available(self, version_tag: &str) -> String {
        match self {
            Self::English => format!("Update available: {version_tag}"),
            Self::Italian => format!("Aggiornamento disponibile: {version_tag}"),
            Self::French => format!("Mise à jour disponible : {version_tag}"),
            Self::German => format!("Update verfügbar: {version_tag}"),
            Self::Spanish => format!("Actualización disponible: {version_tag}"),
        }
    }

    pub fn fmt_up_to_date(self, version: &str) -> String {
        match self {
            Self::English => format!("You are using the latest version (v{version})."),
            Self::Italian => format!("Stai già usando l'ultima versione (v{version})."),
            Self::French => format!("Vous utilisez déjà la dernière version (v{version})."),
            Self::German => format!("Du nutzt bereits die neueste Version (v{version})."),
            Self::Spanish => format!("Ya estás usando la última versión (v{version})."),
        }
    }

    pub fn fmt_update_check_failed(self, err: &str) -> String {
        match self {
            Self::English => format!("Update check failed: {err}"),
            Self::Italian => format!("Controllo aggiornamenti fallito: {err}"),
            Self::French => format!("Échec de vérification des mises à jour : {err}"),
            Self::German => format!("Update-Prüfung fehlgeschlagen: {err}"),
            Self::Spanish => format!("Error al buscar actualizaciones: {err}"),
        }
    }

    pub fn fmt_update_install_failed(self, err: &str) -> String {
        match self {
            Self::English => format!("Update installation failed: {err}"),
            Self::Italian => format!("Installazione aggiornamento fallita: {err}"),
            Self::French => format!("Échec d'installation de la mise à jour : {err}"),
            Self::German => format!("Update-Installation fehlgeschlagen: {err}"),
            Self::Spanish => format!("Error al instalar la actualización: {err}"),
        }
    }
}

pub struct UiStrings {
    pub tab_rules: &'static str,
    pub tab_settings: &'static str,
    pub tab_security: &'static str,
    pub tab_security_alert: &'static str,
    pub subtab_rule_list: &'static str,
    pub subtab_groups: &'static str,
    pub subtab_duplicates: &'static str,
    pub subtab_general: &'static str,
    pub subtab_themes: &'static str,
    pub subtab_shortcuts: &'static str,
    pub section_editor_shortcuts: &'static str,
    pub shortcuts_intro: &'static str,
    pub shortcut_save: &'static str,
    pub shortcut_new_rule: &'static str,
    pub shortcut_search: &'static str,
    pub shortcut_undo: &'static str,
    pub shortcut_redo: &'static str,
    pub shortcut_delete_rule: &'static str,
    pub drag_action_tooltip: &'static str,

    pub status_loading: &'static str,
    pub status_active: &'static str,
    pub status_paused: &'static str,
    pub status_error: &'static str,
    pub status_not_running: &'static str,
    pub status_ipc_error: &'static str,
    pub menu_daemon: &'static str,
    pub menu_refresh: &'static str,
    pub menu_suspend: &'static str,
    pub menu_resume: &'static str,
    pub menu_reload: &'static str,

    pub rules_blocked_invalid: &'static str,
    pub go_to_settings_restore: &'static str,
    pub btn_new: &'static str,
    pub btn_duplicate: &'static str,
    pub label_groups: &'static str,
    pub label_search: &'static str,
    pub filter_all_groups: &'static str,
    pub filter_no_group: &'static str,
    pub search_hint_rules: &'static str,
    pub btn_clear: &'static str,
    pub filter_only_active: &'static str,
    pub filter_only_disabled: &'static str,
    pub empty_rules_filtered: &'static str,
    pub no_group_paren: &'static str,
    pub checkbox_enabled: &'static str,
    pub disabled_no_group_tip: &'static str,
    pub btn_up: &'static str,
    pub btn_down: &'static str,
    pub btn_delete: &'static str,
    pub select_or_create_rule: &'static str,
    pub rule_detail_title: &'static str,
    pub label_name: &'static str,
    pub btn_new_group: &'static str,
    pub label_trigger: &'static str,
    pub btn_record_trigger: &'static str,
    pub btn_record_actions: &'static str,
    pub checkbox_wait_release: &'static str,
    pub label_context: &'static str,
    pub context_mode_global: &'static str,
    pub rule_context_inherit: &'static str,
    pub warning_empty_context_intersection: &'static str,
    pub context_mode_include: &'static str,
    pub context_mode_exclude: &'static str,
    pub label_process_exe: &'static str,
    pub process_exe_hint: &'static str,
    pub btn_choose_app: &'static str,
    pub btn_browse_exe: &'static str,
    pub search_apps_hint: &'static str,
    pub btn_refresh_apps: &'static str,
    pub no_apps_found: &'static str,
    pub modal_record_trigger_title: &'static str,
    pub modal_record_actions_title: &'static str,
    pub record_trigger_hint: &'static str,
    pub record_actions_hint: &'static str,
    pub record_waiting_keys: &'static str,
    pub record_waiting_actions: &'static str,
    pub checkbox_record_mouse_move: &'static str,
    pub record_current_modifiers: &'static str,
    pub record_captured_trigger: &'static str,
    pub record_captured_actions: &'static str,
    pub btn_reset: &'static str,
    pub btn_confirm: &'static str,

    pub groups_title: &'static str,
    pub label_new: &'static str,
    pub group_hint: &'static str,
    pub btn_create: &'static str,
    pub filter_groups_hint: &'static str,
    pub empty_groups: &'static str,
    pub default_suffix: &'static str,
    pub checkbox_group_active: &'static str,
    pub all_groups_active_note: &'static str,
    pub available_rules_title: &'static str,
    pub group_members_title: &'static str,
    pub btn_add_to_group: &'static str,
    pub btn_remove_from_group: &'static str,
    pub btn_select_all: &'static str,
    pub btn_deselect_all: &'static str,
    pub search_hint_name_hotkey: &'static str,
    pub empty_rules_configured: &'static str,
    pub already_in_group_tip: &'static str,
    pub empty_group_members: &'static str,
    pub btn_open: &'static str,

    pub dup_col_keep: &'static str,
    pub dup_col_status: &'static str,
    pub dup_col_trigger: &'static str,
    pub dup_col_groups: &'static str,
    pub status_word_active: &'static str,
    pub status_word_inactive: &'static str,
    pub dup_merge_checkbox: &'static str,

    pub settings_blocked_invalid: &'static str,
    pub section_input_controls: &'static str,
    pub label_kill_switch: &'static str,
    pub btn_restore_default: &'static str,
    pub label_resume_daemon: &'static str,
    pub checkbox_enable_hotkey: &'static str,
    pub label_rate_limit: &'static str,
    pub section_startup_groups: &'static str,
    pub checkbox_all_groups_enabled: &'static str,
    pub startup_groups_note: &'static str,
    pub section_autostart: &'static str,
    pub checkbox_autostart: &'static str,
    pub autostart_note: &'static str,
    pub section_language: &'static str,
    pub label_ui_language: &'static str,
    pub btn_import_config: &'static str,
    pub btn_export_config: &'static str,
    pub select_active_config_hint: &'static str,
    pub checkbox_true_black: &'static str,
    pub true_black_light_tip: &'static str,
    pub section_dark_themes: &'static str,
    pub section_light_themes: &'static str,
    pub section_custom_theme: &'static str,
    pub custom_theme_card_title: &'static str,
    pub btn_edit_theme: &'static str,
    pub btn_import_theme: &'static str,
    pub btn_export_theme: &'static str,
    pub btn_back: &'static str,
    pub btn_reset_custom_theme: &'static str,
    pub theme_editor_title: &'static str,
    pub theme_cat_mode: &'static str,
    pub theme_cat_colors: &'static str,
    pub theme_cat_radii: &'static str,
    pub theme_mode_label: &'static str,
    pub theme_mode_dark: &'static str,
    pub theme_mode_light: &'static str,
    pub color_bg_panel: &'static str,
    pub color_bg_card: &'static str,
    pub color_bg_input: &'static str,
    pub color_bg_widget: &'static str,
    pub color_accent: &'static str,
    pub color_secondary_accent: &'static str,
    pub color_text: &'static str,
    pub color_muted_text: &'static str,
    pub color_border: &'static str,
    pub radius_controls: &'static str,
    pub radius_inner_boxes: &'static str,
    pub radius_cards_menus: &'static str,
    pub radius_modals: &'static str,
    pub toast_theme_imported: &'static str,
    pub toast_theme_exported: &'static str,

    pub section_daemon_status: &'static str,
    pub section_daemon_conflicts: &'static str,
    pub no_daemon_conflicts: &'static str,
    pub daemon_resume_hotkey_failed: &'static str,
    pub btn_open_rule: &'static str,
    pub badge_daemon_skipped: &'static str,
    pub badge_trigger_conflict: &'static str,
    pub badge_trigger_shared: &'static str,
    pub conflict_kill_switch: &'static str,
    pub conflict_resume_hotkey: &'static str,
    pub stat_total_rules: &'static str,
    pub stat_enabled_rules: &'static str,
    pub stat_total_groups: &'static str,
    pub stat_active_groups: &'static str,
    pub stat_active_rules_in_active_groups: &'static str,
    pub daemon_menu_hint: &'static str,
    pub section_config_file: &'static str,
    pub btn_reload_from_disk: &'static str,
    pub btn_open_config_folder: &'static str,
    pub btn_open_logs_folder: &'static str,
    pub section_recent_log_warnings: &'static str,
    pub btn_refresh_logs: &'static str,
    pub no_recent_log_warnings: &'static str,
    pub btn_undo: &'static str,
    pub btn_redo: &'static str,
    pub label_path_prefix: &'static str,
    pub missing_file_note: &'static str,
    pub invalid_file_title: &'static str,
    pub invalid_file_untouched: &'static str,
    pub checkbox_confirm_backup: &'static str,
    pub btn_backup_restore: &'static str,
    pub section_guarantees: &'static str,
    pub guarantee_1: &'static str,
    pub guarantee_2: &'static str,
    pub guarantee_3: &'static str,
    pub guarantee_4: &'static str,
    pub guarantee_5: &'static str,

    pub footer_invalid_disk: &'static str,
    pub footer_valid: &'static str,
    pub footer_unsaved: &'static str,
    pub btn_discard: &'static str,
    pub btn_save: &'static str,

    pub modal_title_delete: &'static str,
    pub modal_title_duplicates: &'static str,
    pub modal_title_overwrite: &'static str,
    pub modal_title_unsaved: &'static str,
    pub modal_exit_unsaved_confirm: &'static str,
    pub btn_cancel: &'static str,
    pub btn_save_short: &'static str,
    pub capture_edit_chip_tip: &'static str,
    pub btn_overwrite: &'static str,
    pub btn_switch_file: &'static str,
    pub btn_close_without_saving: &'static str,
    pub btn_save_and_close: &'static str,

    pub toast_discarded: &'static str,
    pub toast_saved: &'static str,
    pub toast_reloaded_disk: &'static str,
    pub toast_daemon_reloaded: &'static str,
    pub toast_daemon_not_running_save: &'static str,
    pub toast_suspended: &'static str,
    pub toast_suspend_error_state: &'static str,
    pub toast_resumed: &'static str,
    pub toast_resume_error_state: &'static str,
    pub toast_autostart_on: &'static str,
    pub toast_autostart_off: &'static str,
    pub toast_fallback_default: &'static str,
    pub drop_overlay_import: &'static str,
    pub toast_drop_single_file: &'static str,
    pub toast_drop_only_toml: &'static str,
    pub btn_check_updates: &'static str,
    pub btn_checking_updates: &'static str,
    pub section_updates_project: &'static str,
    pub checkbox_auto_check_updates: &'static str,
    pub btn_changelog: &'static str,
    pub btn_github_repo: &'static str,
    pub btn_close: &'static str,
    pub btn_update_now: &'static str,
    pub btn_updating: &'static str,
    pub modal_update_unsaved_confirm: &'static str,
    pub btn_update_without_saving: &'static str,
    pub btn_save_and_update: &'static str,
}

const EN: UiStrings = UiStrings {
    tab_rules: "Rules",
    tab_settings: "Settings",
    tab_security: "Status & security",
    tab_security_alert: "Status & security (!)",
    subtab_rule_list: "Rule list",
    subtab_groups: "Groups",
    subtab_duplicates: "Duplicate rules",
    subtab_general: "General",
    subtab_themes: "Themes",
    subtab_shortcuts: "Shortcuts",
    section_editor_shortcuts: "Editor keyboard shortcuts",
    shortcuts_intro: "Ctrl+Z, Ctrl+Y and Delete trigger only when no text field has keyboard focus.",
    shortcut_save: "Save configuration and reload in daemon",
    shortcut_new_rule: "Create a new rule",
    shortcut_search: "Focus rule search box",
    shortcut_undo: "Undo last change",
    shortcut_redo: "Redo undone change",
    shortcut_delete_rule: "Delete selected rule or group (opens confirmation)",
    drag_action_tooltip: "Drag to reorder action",
    status_loading: "Checking...",
    status_active: "Active",
    status_paused: "Paused",
    status_error: "Error",
    status_not_running: "Not running",
    status_ipc_error: "IPC Error",
    menu_daemon: "Daemon",
    menu_refresh: "Refresh status",
    menu_suspend: "Suspend daemon",
    menu_resume: "Resume daemon",
    menu_reload: "Reload in daemon",
    rules_blocked_invalid: "Rule editing locked: config.toml on disk is invalid (fail-closed).",
    go_to_settings_restore: "Open Settings to backup and restore",
    btn_new: "Create",
    btn_duplicate: "Duplicate",
    label_groups: "Groups:",
    label_search: "Search:",
    filter_all_groups: "All groups",
    filter_no_group: "No group",
    search_hint_rules: "name, group, hotkey",
    btn_clear: "Clear",
    filter_only_active: "Active only",
    filter_only_disabled: "Disabled only",
    empty_rules_filtered: "No rules found.",
    no_group_paren: "(no group)",
    checkbox_enabled: "Enabled",
    disabled_no_group_tip: "Assign at least one group before enabling this rule",
    btn_up: "Up",
    btn_down: "Down",
    btn_delete: "Delete",
    select_or_create_rule: "Select a rule on the left or click \"Create\".",
    rule_detail_title: "Rule Details",
    label_name: "Name:",
    btn_new_group: "Create",
    label_trigger: "Trigger:",
    btn_record_trigger: "Record",
    btn_record_actions: "Record",
    checkbox_wait_release: "Wait for trigger keys release before running",
    label_context: "Context:",
    context_mode_global: "Everywhere (global)",
    rule_context_inherit: "Everywhere / inherit from groups",
    warning_empty_context_intersection: "Warning: this filter never overlaps with the rule's active group context (rule stays inactive).",
    context_mode_include: "Only in selected apps",
    context_mode_exclude: "Everywhere except",
    label_process_exe: "Process (.exe):",
    process_exe_hint: "e.g. code.exe, chrome.exe",
    btn_choose_app: "Apps...",
    btn_browse_exe: "Browse .exe...",
    search_apps_hint: "Filter installed/running apps...",
    btn_refresh_apps: "Refresh",
    no_apps_found: "No matching applications",
    modal_record_trigger_title: "Record trigger",
    modal_record_actions_title: "Record actions",
    record_trigger_hint: "Press the key combination to use as trigger (Esc, Tab, Space, arrows supported). If an active daemon rule intercepts it, pause the daemon first.",
    record_actions_hint: "Press keys or click/scroll with the mouse inside the box below. Clicks and key releases collapse automatically; held inputs stay Down/Up.",
    record_waiting_keys: "Waiting for key press...",
    record_waiting_actions: "Waiting for keys, mouse clicks, or scroll inside this box...",
    checkbox_record_mouse_move: "Include relative mouse movements inside box",
    record_current_modifiers: "Held modifiers:",
    record_captured_trigger: "Captured trigger:",
    record_captured_actions: "Recorded actions:",
    btn_reset: "Reset",
    btn_confirm: "Confirm",
    groups_title: "Groups",
    label_new: "Name:",
    group_hint: "e.g. work_1",
    btn_create: "Create",
    filter_groups_hint: "filter groups",
    empty_groups: "No groups found.",
    default_suffix: "default",
    checkbox_group_active: "Active",
    all_groups_active_note: "(mode: all groups active)",
    available_rules_title: "Available rules",
    group_members_title: "Rules in group",
    btn_add_to_group: "Add",
    btn_remove_from_group: "Remove",
    btn_select_all: "Select all",
    btn_deselect_all: "Deselect all",
    search_hint_name_hotkey: "name or hotkey",
    empty_rules_configured: "No rules configured.",
    already_in_group_tip: "Rule already belongs to this group",
    empty_group_members: "No rules in this group.",
    btn_open: "Open",
    dup_col_keep: "Rule to keep",
    dup_col_status: "Status",
    dup_col_trigger: "Trigger",
    dup_col_groups: "Groups",
    status_word_active: "Active",
    status_word_inactive: "Disabled",
    dup_merge_checkbox: "Replace the rules to delete with the kept rule in the groups they belong to",
    settings_blocked_invalid: "Configuration settings locked: restore config.toml in the Configuration file section below first.",
    section_input_controls: "Hotkeys & input limits",
    label_kill_switch: "Kill-Switch:",
    btn_restore_default: "Restore default",
    label_resume_daemon: "Resume daemon:",
    checkbox_enable_hotkey: "Enable hotkey",
    label_rate_limit: "Rate limit (events/sec):",
    section_startup_groups: "Startup groups",
    checkbox_all_groups_enabled: "All groups enabled",
    startup_groups_note: "If you disable every specific group, 'default' stays active automatically. Create new groups from Rules -> Groups.",
    section_autostart: "Start with Windows",
    checkbox_autostart: "Start RustBind on Windows sign-in",
    autostart_note: "Registers the daemon in HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run without admin privileges.",
    section_language: "Language",
    label_ui_language: "Interface language:",
    btn_import_config: "Import configuration...",
    btn_export_config: "Export configuration...",
    select_active_config_hint: "Select the active configuration file (one at a time):",
    checkbox_true_black: "True Black (pure #000000 backgrounds for dark themes)",
    true_black_light_tip: "Select a dark theme to enable True Black mode",
    section_dark_themes: "Dark themes",
    section_light_themes: "Light themes",
    section_custom_theme: "Custom theme",
    custom_theme_card_title: "Custom theme",
    btn_edit_theme: "Edit",
    btn_import_theme: "Import theme...",
    btn_export_theme: "Export theme...",
    btn_back: "Back",
    btn_reset_custom_theme: "Reset defaults",
    theme_editor_title: "Theme Editor",
    theme_cat_mode: "Mode & Base Options",
    theme_cat_colors: "Color Scheme",
    theme_cat_radii: "Border Radius",
    theme_mode_label: "Base mode:",
    theme_mode_dark: "Dark",
    theme_mode_light: "Light",
    color_bg_panel: "Window / panel background",
    color_bg_card: "Card & popup background",
    color_bg_input: "Text input background",
    color_bg_widget: "Button & widget background",
    color_accent: "Primary accent",
    color_secondary_accent: "Secondary accent",
    color_text: "Base text",
    color_muted_text: "Secondary text",
    color_border: "Borders & dividers",
    radius_controls: "Buttons, inputs, selects & keycaps:",
    radius_inner_boxes: "Inner containers (e.g. duplicate actions box):",
    radius_cards_menus: "Cards, toasts, dropdowns & color picker:",
    radius_modals: "Modals & floating windows:",
    toast_theme_imported: "Custom theme imported and activated.",
    toast_theme_exported: "Custom theme exported.",
    section_daemon_status: "Daemon Connection Status",
    section_daemon_conflicts: "Daemon hotkey registration conflicts",
    no_daemon_conflicts: "No hotkey registration conflicts reported by the daemon.",
    daemon_resume_hotkey_failed: "Resume hotkey could not be registered in Windows (already in use).",
    btn_open_rule: "Open rule",
    badge_daemon_skipped: "Skipped by daemon",
    badge_trigger_conflict: "Trigger conflict",
    badge_trigger_shared: "Shared trigger",
    conflict_kill_switch: "Matches the Kill-Switch hotkey: this rule will always be ignored.",
    conflict_resume_hotkey: "Matches the Resume hotkey: this rule will be ignored.",
    stat_total_rules: "Registered rules:",
    stat_enabled_rules: "Active rules:",
    stat_total_groups: "Registered groups:",
    stat_active_groups: "Active groups:",
    stat_active_rules_in_active_groups: "Active rules in active groups:",
    daemon_menu_hint: "Use the \"Daemon\" menu in the top bar to refresh status, suspend/resume or reload config.",
    section_config_file: "Configuration file",
    btn_reload_from_disk: "Reload from disk",
    btn_open_config_folder: "Open config folder",
    btn_open_logs_folder: "Open log folder",
    section_recent_log_warnings: "Recent daemon log warnings / errors",
    btn_refresh_logs: "Refresh logs",
    no_recent_log_warnings: "No recent warnings or errors found in rustbind.log.",
    btn_undo: "Undo",
    btn_redo: "Redo",
    label_path_prefix: "Path",
    missing_file_note: "File absent: in-memory draft (nothing written to disk until Save configuration).",
    invalid_file_title: "Invalid configuration file (fail-closed mode)",
    invalid_file_untouched: "The original file on disk was not modified, moved or overwritten.",
    checkbox_confirm_backup: "I confirm creating a backup copy and restoring an empty default configuration",
    btn_backup_restore: "Create backup and restore",
    section_guarantees: "Security, Privacy & IPC Guarantees (v1)",
    guarantee_1: "• Official Windows APIs only (RegisterHotKey, SendInput, Shell_NotifyIconW); no WH_KEYBOARD_LL hook and no LaunchProcess actions.",
    guarantee_2: "• Strict fail-closed behavior: if config.toml is invalid, no rule runs and neither IPC Resume nor resume hotkey can exit Error state.",
    guarantee_3: "• Local Named Pipe DACL-restricted to the current user SID: GetStatus, Suspend, ReloadConfig and Resume only.",
    guarantee_4: "• No SendText payloads or configuration values are ever logged or exposed in error messages.",
    guarantee_5: "• Closing only this window keeps the daemon in the tray; selecting Exit from the tray closes the UI too.",
    footer_invalid_disk: "File on disk is invalid — open Settings for backup and restore",
    footer_valid: "Configuration valid",
    footer_unsaved: "Unsaved changes",
    btn_discard: "Discard changes",
    btn_save: "Save configuration",
    modal_title_delete: "Confirm deletion",
    modal_title_duplicates: "Confirm duplicate removal",
    modal_title_overwrite: "Confirm overwrite",
    modal_title_unsaved: "Unsaved changes",
    modal_exit_unsaved_confirm: "You have unsaved changes.\nClose without saving?",
    btn_cancel: "Cancel",
    btn_save_short: "Save",
    capture_edit_chip_tip: "Click to edit",
    btn_overwrite: "Overwrite",
    btn_switch_file: "Switch file",
    btn_close_without_saving: "Close without saving",
    btn_save_and_close: "Save and close",
    toast_discarded: "Unsaved changes discarded.",
    toast_saved: "Configuration safely saved to disk.",
    toast_reloaded_disk: "Configuration reloaded from disk.",
    toast_daemon_reloaded: "Configuration saved and reloaded in daemon.",
    toast_daemon_not_running_save: "Saved to disk, but daemon is not running for live reload.",
    toast_suspended: "Daemon suspended successfully.",
    toast_suspend_error_state: "Daemon is in Error state: no change applied.",
    toast_resumed: "Daemon resumed successfully.",
    toast_resume_error_state: "Daemon is in Error state: cannot resume (fail-closed).",
    toast_autostart_on: "Start on Windows sign-in enabled.",
    toast_autostart_off: "Start on Windows sign-in disabled.",
    toast_fallback_default: "At least one group must stay active: fallback to 'default'.",
    drop_overlay_import: "Drop .toml file to import configuration or custom theme",
    toast_drop_single_file: "Drop a single .toml file at a time.",
    toast_drop_only_toml: "Only .toml configuration files can be imported.",
    btn_check_updates: "Check for updates",
    btn_checking_updates: "Checking...",
    section_updates_project: "Updates & Project",
    checkbox_auto_check_updates: "Automatically check for updates",
    btn_changelog: "Changelog",
    btn_github_repo: "GitHub repository",
    btn_close: "Close",
    btn_update_now: "Update",
    btn_updating: "Updating...",
    modal_update_unsaved_confirm: "You have unsaved changes.\nApply the update anyway?",
    btn_update_without_saving: "Update without saving",
    btn_save_and_update: "Save and update",
};

const IT: UiStrings = UiStrings {
    tab_rules: "Regole",
    tab_settings: "Impostazioni",
    tab_security: "Stato e sicurezza",
    tab_security_alert: "Stato e sicurezza (!)",
    subtab_rule_list: "Elenco regole",
    subtab_groups: "Gruppi",
    subtab_duplicates: "Regole duplicate",
    subtab_general: "Generali",
    subtab_themes: "Temi",
    subtab_shortcuts: "Scorciatoie",
    section_editor_shortcuts: "Scorciatoie da tastiera dell'editor",
    shortcuts_intro: "Ctrl+Z, Ctrl+Y e Canc si attivano solo quando non stai scrivendo in un campo di testo.",
    shortcut_save: "Salva la configurazione e ricarica nel daemon",
    shortcut_new_rule: "Crea una nuova regola",
    shortcut_search: "Porta il focus sulla ricerca regole",
    shortcut_undo: "Annulla l'ultima modifica (Undo)",
    shortcut_redo: "Ripristina la modifica annullata (Redo)",
    shortcut_delete_rule: "Elimina la regola o il gruppo selezionato (apre conferma)",
    drag_action_tooltip: "Trascina per riordinare l'azione",
    status_loading: "Interrogazione in corso...",
    status_active: "Attivo",
    status_paused: "In Pausa",
    status_error: "Errore",
    status_not_running: "Non in esecuzione",
    status_ipc_error: "Errore IPC",
    menu_daemon: "Daemon",
    menu_refresh: "Aggiorna stato",
    menu_suspend: "Sospendi daemon",
    menu_resume: "Riprendi daemon",
    menu_reload: "Ricarica nel daemon",
    rules_blocked_invalid: "Modifica regole bloccata: il file config.toml su disco non è valido (fail-closed).",
    go_to_settings_restore: "Vai a Impostazioni per backup e ripristino",
    btn_new: "Crea",
    btn_duplicate: "Duplica",
    label_groups: "Gruppi:",
    label_search: "Cerca:",
    filter_all_groups: "Tutti i gruppi",
    filter_no_group: "Nessun gruppo",
    search_hint_rules: "nome, gruppo, hotkey",
    btn_clear: "Pulisci",
    filter_only_active: "Solo attive",
    filter_only_disabled: "Solo disattivate",
    empty_rules_filtered: "Nessuna regola trovata.",
    no_group_paren: "(nessun gruppo)",
    checkbox_enabled: "Attiva",
    disabled_no_group_tip: "Assegna almeno un gruppo alla regola prima di attivarla",
    btn_up: "Su",
    btn_down: "Giù",
    btn_delete: "Elimina",
    select_or_create_rule: "Seleziona una regola a sinistra o clicca \"Crea\".",
    rule_detail_title: "Dettaglio Regola",
    label_name: "Nome:",
    btn_new_group: "Crea",
    label_trigger: "Trigger:",
    btn_record_trigger: "Registra",
    btn_record_actions: "Registra",
    checkbox_wait_release: "Attendi il rilascio dei tasti prima di eseguire il trigger",
    label_context: "Contesto:",
    context_mode_global: "Ovunque (globale)",
    rule_context_inherit: "Ovunque / eredita dai gruppi",
    warning_empty_context_intersection: "Attenzione: questo filtro non si sovrappone mai al contesto dei gruppi attivi (regola inattiva).",
    context_mode_include: "Solo nelle app selezionate",
    context_mode_exclude: "Ovunque tranne in",
    label_process_exe: "Processo (.exe):",
    process_exe_hint: "es. code.exe, chrome.exe",
    btn_choose_app: "Scegli app...",
    btn_browse_exe: "Sfoglia .exe...",
    search_apps_hint: "Filtra app installate o aperte...",
    btn_refresh_apps: "Aggiorna",
    no_apps_found: "Nessuna applicazione trovata",
    modal_record_trigger_title: "Registra trigger",
    modal_record_actions_title: "Registra azioni",
    record_trigger_hint: "Premi la combinazione di tasti da usare come trigger (supportati anche Esc, Tab, Spazio, frecce). Se una regola attiva nel daemon la intercetta, sospendilo prima.",
    record_actions_hint: "Premi tasti oppure usa click e rotella del mouse dentro il riquadro qui sotto. Pressione e rilascio diventano Click; gli input mantenuti restano Down/Up.",
    record_waiting_keys: "In attesa della pressione dei tasti...",
    record_waiting_actions: "In attesa di tasti, click o rotella del mouse in questo riquadro...",
    checkbox_record_mouse_move: "Includi spostamenti relativi del mouse nel riquadro",
    record_current_modifiers: "Modificatori premuti:",
    record_captured_trigger: "Trigger rilevato:",
    record_captured_actions: "Azioni registrate:",
    btn_reset: "Resetta",
    btn_confirm: "Conferma",
    groups_title: "Gruppi",
    label_new: "Nome:",
    group_hint: "es. lavoro_1",
    btn_create: "Crea",
    filter_groups_hint: "filtra gruppi",
    empty_groups: "Nessun gruppo trovato.",
    default_suffix: "predefinito",
    checkbox_group_active: "Attivo",
    all_groups_active_note: "(modalità: tutti i gruppi attivi)",
    available_rules_title: "Regole disponibili",
    group_members_title: "Regole nel gruppo",
    btn_add_to_group: "Aggiungi",
    btn_remove_from_group: "Rimuovi",
    btn_select_all: "Seleziona tutte",
    btn_deselect_all: "Deseleziona tutte",
    search_hint_name_hotkey: "nome o hotkey",
    empty_rules_configured: "Nessuna regola configurata.",
    already_in_group_tip: "Regola già presente in questo gruppo",
    empty_group_members: "Nessuna regola presente in questo gruppo.",
    btn_open: "Apri",
    dup_col_keep: "Regola da mantenere",
    dup_col_status: "Stato",
    dup_col_trigger: "Trigger",
    dup_col_groups: "Gruppi",
    status_word_active: "Attiva",
    status_word_inactive: "Disattiva",
    dup_merge_checkbox: "Sostituisci le regole da eliminare con la regola mantenuta, nei gruppi in cui sono presenti",
    settings_blocked_invalid: "Impostazioni configurazione bloccate: ripristina prima config.toml nella sezione File di configurazione qui sotto.",
    section_input_controls: "Scorciatoie e limiti input",
    label_kill_switch: "Kill-Switch:",
    btn_restore_default: "Ripristina default",
    label_resume_daemon: "Riprendi daemon:",
    checkbox_enable_hotkey: "Abilita hotkey",
    label_rate_limit: "Limite eventi/sec:",
    section_startup_groups: "Gruppi all'avvio",
    checkbox_all_groups_enabled: "Tutti i gruppi abilitati",
    startup_groups_note: "Se disattivi tutti i gruppi specifici, il sistema mantiene automaticamente attivo 'default'. Crea nuovi gruppi dal tab Regole -> Gruppi.",
    section_autostart: "Avvio con Windows",
    checkbox_autostart: "Avvia RustBind all'accesso a Windows",
    autostart_note: "Registra il daemon in HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run senza privilegi amministrativi.",
    section_language: "Lingua",
    label_ui_language: "Lingua interfaccia:",
    btn_import_config: "Importa configurazione...",
    btn_export_config: "Esporta configurazione...",
    select_active_config_hint: "Seleziona il file di configurazione attivo (solo uno alla volta):",
    checkbox_true_black: "True Black (neri assoluti #000000 per i temi scuri)",
    true_black_light_tip: "Seleziona un tema scuro per attivare la modalità True Black",
    section_dark_themes: "Temi scuri",
    section_light_themes: "Temi chiari",
    section_custom_theme: "Tema custom",
    custom_theme_card_title: "Tema custom",
    btn_edit_theme: "Modifica",
    btn_import_theme: "Importa tema...",
    btn_export_theme: "Esporta tema...",
    btn_back: "Indietro",
    btn_reset_custom_theme: "Ripristina predefiniti",
    theme_editor_title: "Editor Tema",
    theme_cat_mode: "Modalità e opzioni base",
    theme_cat_colors: "Schema colori",
    theme_cat_radii: "Border radius componenti",
    theme_mode_label: "Modalità base:",
    theme_mode_dark: "Scuro",
    theme_mode_light: "Chiaro",
    color_bg_panel: "Sfondo finestra / pannello",
    color_bg_card: "Sfondo card e popup",
    color_bg_input: "Sfondo campi di testo",
    color_bg_widget: "Sfondo pulsanti e widget",
    color_accent: "Accento primario",
    color_secondary_accent: "Accento secondario",
    color_text: "Testo principale",
    color_muted_text: "Testo secondario",
    color_border: "Bordi e separatori",
    radius_controls: "Pulsanti, input, select e keycap:",
    radius_inner_boxes: "Box interni (es. azioni duplicate):",
    radius_cards_menus: "Card, toast, tendine, color picker e menu:",
    radius_modals: "Modali e finestre:",
    toast_theme_imported: "Tema custom importato e attivato.",
    toast_theme_exported: "Tema custom esportato.",
    section_daemon_status: "Stato Connessione Daemon",
    section_daemon_conflicts: "Conflitti di registrazione nel daemon",
    no_daemon_conflicts: "Nessun conflitto di registrazione segnalato dal daemon.",
    daemon_resume_hotkey_failed: "L'hotkey di ripresa configurata non è stata registrata in Windows (già occupata).",
    btn_open_rule: "Apri regola",
    badge_daemon_skipped: "Saltata dal daemon",
    badge_trigger_conflict: "Conflitto trigger",
    badge_trigger_shared: "Trigger condiviso",
    conflict_kill_switch: "Coincide con il Kill-Switch: questa regola verrà ignorata dal daemon.",
    conflict_resume_hotkey: "Coincide con l'hotkey di ripresa: questa regola verrà ignorata.",
    stat_total_rules: "Regole registrate:",
    stat_enabled_rules: "Regole attive:",
    stat_total_groups: "Gruppi registrati:",
    stat_active_groups: "Gruppi attivi:",
    stat_active_rules_in_active_groups: "Regole attive in gruppi attivi:",
    daemon_menu_hint: "Usa il menu \"Daemon\" nella barra superiore per aggiornare lo stato, sospendere/riprendere o ricaricare la configurazione.",
    section_config_file: "File di configurazione",
    btn_reload_from_disk: "Ricarica da disco",
    btn_open_config_folder: "Apri cartella config",
    btn_open_logs_folder: "Apri cartella log",
    section_recent_log_warnings: "Ultimi avvisi / errori nei log del daemon",
    btn_refresh_logs: "Aggiorna log",
    no_recent_log_warnings: "Nessun avviso o errore recente rilevato in rustbind.log.",
    btn_undo: "Annulla azione",
    btn_redo: "Ripeti",
    label_path_prefix: "Percorso",
    missing_file_note: "File assente: bozza iniziale in memoria (nessun file scritto fino al clic su Salva configurazione).",
    invalid_file_title: "File di configurazione non valido (modalità fail-closed)",
    invalid_file_untouched: "Il file originale su disco non è stato toccato, spostato né sovrascritto.",
    checkbox_confirm_backup: "Confermo di voler creare una copia di backup e sovrascrivere con una configurazione vuota",
    btn_backup_restore: "Crea backup e ripristina",
    section_guarantees: "Garanzie di Sicurezza, Privacy & IPC (v1)",
    guarantee_1: "• API ufficiali Windows (RegisterHotKey, SendInput, Shell_NotifyIconW); nessun hook WH_KEYBOARD_LL né avvio processi (LaunchProcess) nelle regole.",
    guarantee_2: "• Fail-closed rigoroso: se config.toml è non valido, nessuna regola parte e né Resume IPC né l'hotkey di ripresa possono uscire dallo stato Errore.",
    guarantee_3: "• Named Pipe locale DACL-restricted al solo SID dell'utente corrente: GetStatus, Suspend, ReloadConfig e Resume.",
    guarantee_4: "• Nessun testo SendText o valore di configurazione viene scritto nei log o esposto nei messaggi di errore.",
    guarantee_5: "• Chiudendo solo questa finestra il daemon resta attivo nella tray bar; scegliendo 'Esci' dal menu tray si chiude anche la UI.",
    footer_invalid_disk: "File su disco non valido — apri Impostazioni per backup e ripristino",
    footer_valid: "Configurazione valida",
    footer_unsaved: "Modifiche non salvate",
    btn_discard: "Annulla modifiche",
    btn_save: "Salva configurazione",
    modal_title_delete: "Conferma eliminazione",
    modal_title_duplicates: "Conferma eliminazione duplicati",
    modal_title_overwrite: "Conferma sovrascrittura",
    modal_title_unsaved: "Modifiche non salvate",
    modal_exit_unsaved_confirm: "Ci sono modifiche non salvate.\nChiudere senza salvare?",
    btn_cancel: "Annulla",
    btn_save_short: "Salva",
    capture_edit_chip_tip: "Clicca per modificare",
    btn_overwrite: "Sovrascrivi",
    btn_switch_file: "Cambia file",
    btn_close_without_saving: "Chiudi senza salvare",
    btn_save_and_close: "Salva e chiudi",
    toast_discarded: "Modifiche non salvate annullate.",
    toast_saved: "Configurazione salvata in modo sicuro su disco.",
    toast_reloaded_disk: "Configurazione ricaricata dal disco.",
    toast_daemon_reloaded: "Configurazione salvata e ricaricata nel daemon.",
    toast_daemon_not_running_save: "Salvato su disco, ma daemon non in esecuzione per la ricarica immediata.",
    toast_suspended: "Daemon sospeso con successo.",
    toast_suspend_error_state: "Il daemon è in stato Errore: nessuna modifica applicata.",
    toast_resumed: "Daemon ripreso con successo.",
    toast_resume_error_state: "Il daemon è in stato Errore: impossibile riprendere (fail-closed).",
    toast_autostart_on: "Avvio automatico all'accesso a Windows abilitato.",
    toast_autostart_off: "Avvio automatico all'accesso a Windows disabilitato.",
    toast_fallback_default: "Almeno un gruppo deve restare attivo: fallback su 'default'.",
    drop_overlay_import: "Rilascia il file .toml per importare configurazione o tema custom",
    toast_drop_single_file: "Trascina un solo file .toml alla volta.",
    toast_drop_only_toml: "È possibile importare solo file di configurazione .toml.",
    btn_check_updates: "Cerca aggiornamenti",
    btn_checking_updates: "Verifica...",
    section_updates_project: "Aggiornamenti & Progetto",
    checkbox_auto_check_updates: "Controlla automaticamente gli aggiornamenti",
    btn_changelog: "Changelog",
    btn_github_repo: "Progetto GitHub",
    btn_close: "Chiudi",
    btn_update_now: "Aggiorna",
    btn_updating: "Aggiornamento...",
    modal_update_unsaved_confirm: "Sono presenti modifiche non salvate.\nAggiornare comunque?",
    btn_update_without_saving: "Aggiorna senza salvare",
    btn_save_and_update: "Salva e aggiorna",
};

const FR: UiStrings = UiStrings {
    tab_rules: "Règles",
    tab_settings: "Paramètres",
    tab_security: "État et sécurité",
    tab_security_alert: "État et sécurité (!)",
    subtab_rule_list: "Liste des règles",
    subtab_groups: "Groupes",
    subtab_duplicates: "Règles en double",
    subtab_general: "Général",
    subtab_themes: "Thèmes",
    subtab_shortcuts: "Raccourcis",
    section_editor_shortcuts: "Raccourcis clavier de l'éditeur",
    shortcuts_intro: "Ctrl+Z, Ctrl+Y et Suppr s'activent uniquement hors champ de saisie texte.",
    shortcut_save: "Enregistrer et recharger dans le démon",
    shortcut_new_rule: "Créer une nouvelle règle",
    shortcut_search: "Activer la recherche de règles",
    shortcut_undo: "Annuler la dernière modification",
    shortcut_redo: "Rétablir la modification annulée",
    shortcut_delete_rule: "Supprimer la règle ou le groupe sélectionné (avec confirmation)",
    drag_action_tooltip: "Glisser pour réorganiser",
    status_loading: "Vérification...",
    status_active: "Actif",
    status_paused: "En pause",
    status_error: "Erreur",
    status_not_running: "Arrêté",
    status_ipc_error: "Erreur IPC",
    menu_daemon: "Démon",
    menu_refresh: "Actualiser l'état",
    menu_suspend: "Suspendre le démon",
    menu_resume: "Reprendre le démon",
    menu_reload: "Recharger dans le démon",
    rules_blocked_invalid: "Édition verrouillée : config.toml sur le disque est invalide (fail-closed).",
    go_to_settings_restore: "Ouvrir Paramètres pour sauvegarder et restaurer",
    btn_new: "Créer",
    btn_duplicate: "Dupliquer",
    label_groups: "Groupes :",
    label_search: "Rechercher :",
    filter_all_groups: "Tous les groupes",
    filter_no_group: "Sans groupe",
    search_hint_rules: "nom, groupe, raccourci",
    btn_clear: "Effacer",
    filter_only_active: "Actives uniquement",
    filter_only_disabled: "Désactivées uniquement",
    empty_rules_filtered: "Aucune règle trouvée.",
    no_group_paren: "(aucun groupe)",
    checkbox_enabled: "Active",
    disabled_no_group_tip: "Attribuez au moins un groupe avant d'activer cette règle",
    btn_up: "Monter",
    btn_down: "Descendre",
    btn_delete: "Supprimer",
    select_or_create_rule: "Sélectionnez une règle à gauche ou cliquez sur « Créer ».",
    rule_detail_title: "Détails de la règle",
    label_name: "Nom :",
    btn_new_group: "Créer",
    label_trigger: "Déclencheur :",
    btn_record_trigger: "Enregistrer",
    btn_record_actions: "Enregistrer",
    checkbox_wait_release: "Attendre le relâchement des touches avant d'exécuter",
    label_context: "Contexte :",
    context_mode_global: "Partout (global)",
    rule_context_inherit: "Partout / hériter des groupes",
    warning_empty_context_intersection: "Attention : ce filtre ne recouvre jamais le contexte des groupes actifs (règle inactive).",
    context_mode_include: "Uniquement dans les apps",
    context_mode_exclude: "Partout sauf dans",
    label_process_exe: "Processus (.exe) :",
    process_exe_hint: "ex. code.exe, chrome.exe",
    btn_choose_app: "Choisir...",
    btn_browse_exe: "Parcourir .exe...",
    search_apps_hint: "Filtrer les applications...",
    btn_refresh_apps: "Actualiser",
    no_apps_found: "Aucune application trouvée",
    modal_record_trigger_title: "Enregistrer le déclencheur",
    modal_record_actions_title: "Enregistrer les actions",
    record_trigger_hint: "Appuyez sur la combinaison souhaitée (Échap, Tab, Espace, flèches inclus). Si une règle active l'intercepte, suspendez d'abord le démon.",
    record_actions_hint: "Appuyez sur des touches ou utilisez clics/molette dans le cadre ci-dessous. Appui+relâchement devient Click ; les entrées maintenues restent Down/Up.",
    record_waiting_keys: "En attente d'une pression...",
    record_waiting_actions: "En attente de touches, clics ou molette dans ce cadre...",
    checkbox_record_mouse_move: "Inclure les déplacements relatifs de la souris",
    record_current_modifiers: "Modificateurs maintenus :",
    record_captured_trigger: "Déclencheur capturé :",
    record_captured_actions: "Actions enregistrées :",
    btn_reset: "Réinitialiser",
    btn_confirm: "Confirmer",
    groups_title: "Groupes",
    label_new: "Nom :",
    group_hint: "ex. travail_1",
    btn_create: "Créer",
    filter_groups_hint: "filtrer les groupes",
    empty_groups: "Aucun groupe trouvé.",
    default_suffix: "par défaut",
    checkbox_group_active: "Actif",
    all_groups_active_note: "(mode : tous les groupes actifs)",
    available_rules_title: "Règles disponibles",
    group_members_title: "Règles du groupe",
    btn_add_to_group: "Ajouter",
    btn_remove_from_group: "Retirer",
    btn_select_all: "Tout sélectionner",
    btn_deselect_all: "Tout désélectionner",
    search_hint_name_hotkey: "nom ou raccourci",
    empty_rules_configured: "Aucune règle configurée.",
    already_in_group_tip: "La règle appartient déjà à ce groupe",
    empty_group_members: "Aucune règle dans ce groupe.",
    btn_open: "Ouvrir",
    dup_col_keep: "Règle à conserver",
    dup_col_status: "État",
    dup_col_trigger: "Déclencheur",
    dup_col_groups: "Groupes",
    status_word_active: "Active",
    status_word_inactive: "Désactivée",
    dup_merge_checkbox: "Remplacer les règles à supprimer par la règle conservée dans leurs groupes",
    settings_blocked_invalid: "Paramètres verrouillés : restaurez d'abord config.toml dans la section Fichier de configuration ci-dessous.",
    section_input_controls: "Raccourcis et limites d'entrée",
    label_kill_switch: "Kill-Switch :",
    btn_restore_default: "Réinitialiser",
    label_resume_daemon: "Reprendre le démon :",
    checkbox_enable_hotkey: "Activer le raccourci",
    label_rate_limit: "Limite événements/s :",
    section_startup_groups: "Groupes au démarrage",
    checkbox_all_groups_enabled: "Tous les groupes activés",
    startup_groups_note: "Si vous désactivez tous les groupes spécifiques, 'default' reste actif automatiquement. Créez des groupes dans Règles -> Groupes.",
    section_autostart: "Démarrage avec Windows",
    checkbox_autostart: "Lancer RustBind à l'ouverture de session Windows",
    autostart_note: "Enregistre le démon dans HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run sans droits administrateur.",
    section_language: "Langue",
    label_ui_language: "Langue de l'interface :",
    btn_import_config: "Importer une configuration...",
    btn_export_config: "Exporter la configuration...",
    select_active_config_hint: "Sélectionnez le fichier de configuration actif (un seul à la fois) :",
    checkbox_true_black: "True Black (noir pur #000000 pour les thèmes sombres)",
    true_black_light_tip: "Choisissez un thème sombre pour activer True Black",
    section_dark_themes: "Thèmes sombres",
    section_light_themes: "Thèmes clairs",
    section_custom_theme: "Thème personnalisé",
    custom_theme_card_title: "Thème personnalisé",
    btn_edit_theme: "Modifier",
    btn_import_theme: "Importer un thème...",
    btn_export_theme: "Exporter le thème...",
    btn_back: "Retour",
    btn_reset_custom_theme: "Réinitialiser",
    theme_editor_title: "Éditeur de thème",
    theme_cat_mode: "Mode et options de base",
    theme_cat_colors: "Palette de couleurs",
    theme_cat_radii: "Rayons de bordure",
    theme_mode_label: "Mode de base :",
    theme_mode_dark: "Sombre",
    theme_mode_light: "Clair",
    color_bg_panel: "Arrière-plan fenêtre / panneau",
    color_bg_card: "Arrière-plan cartes et popups",
    color_bg_input: "Arrière-plan champs de saisie",
    color_bg_widget: "Arrière-plan boutons et contrôles",
    color_accent: "Accent principal",
    color_secondary_accent: "Accent secondaire",
    color_text: "Texte principal",
    color_muted_text: "Texte secondaire",
    color_border: "Bordures et séparateurs",
    radius_controls: "Boutons, champs, listes et touches :",
    radius_inner_boxes: "Cadres internes (ex. actions dupliquées) :",
    radius_cards_menus: "Cartes, notifications, menus et sélecteur :",
    radius_modals: "Fenêtres modales et flottantes :",
    toast_theme_imported: "Thème personnalisé importé et activé.",
    toast_theme_exported: "Thème personnalisé exporté.",
    section_daemon_status: "État de connexion du démon",
    section_daemon_conflicts: "Conflits d'enregistrement dans le démon",
    no_daemon_conflicts: "Aucun conflit d'enregistrement signalé par le démon.",
    daemon_resume_hotkey_failed: "Le raccourci de reprise n'a pas pu être enregistré sous Windows (déjà occupé).",
    btn_open_rule: "Ouvrir la règle",
    badge_daemon_skipped: "Ignorée par le démon",
    badge_trigger_conflict: "Conflit déclencheur",
    badge_trigger_shared: "Déclencheur partagé",
    conflict_kill_switch: "Identique au Kill-Switch : cette règle sera ignorée par le démon.",
    conflict_resume_hotkey: "Identique au raccourci de reprise : cette règle sera ignorée.",
    stat_total_rules: "Règles enregistrées :",
    stat_enabled_rules: "Règles actives :",
    stat_total_groups: "Groupes enregistrés :",
    stat_active_groups: "Groupes actifs :",
    stat_active_rules_in_active_groups: "Règles actives dans les groupes actifs :",
    daemon_menu_hint: "Utilisez le menu « Démon » en haut pour actualiser, suspendre/reprendre ou recharger la configuration.",
    section_config_file: "Fichier de configuration",
    btn_reload_from_disk: "Recharger depuis le disque",
    btn_open_config_folder: "Ouvrir dossier config",
    btn_open_logs_folder: "Ouvrir dossier logs",
    section_recent_log_warnings: "Derniers avertissements / erreurs du démon",
    btn_refresh_logs: "Actualiser logs",
    no_recent_log_warnings: "Aucun avertissement ou erreur récent dans rustbind.log.",
    btn_undo: "Annuler",
    btn_redo: "Rétablir",
    label_path_prefix: "Chemin",
    missing_file_note: "Fichier absent : brouillon en mémoire (rien n'est écrit tant que vous n'enregistrez pas).",
    invalid_file_title: "Fichier de configuration invalide (mode fail-closed)",
    invalid_file_untouched: "Le fichier original sur le disque n'a été ni modifié, ni déplacé, ni écrasé.",
    checkbox_confirm_backup: "Je confirme créer une sauvegarde et restaurer une configuration vide par défaut",
    btn_backup_restore: "Créer une sauvegarde et restaurer",
    section_guarantees: "Garanties de sécurité, confidentialité et IPC (v1)",
    guarantee_1: "• API Windows officielles uniquement (RegisterHotKey, SendInput, Shell_NotifyIconW) ; aucun hook WH_KEYBOARD_LL ni LaunchProcess.",
    guarantee_2: "• Mode fail-closed strict : si config.toml est invalide, aucune règle ne démarre et ni Resume IPC ni le raccourci ne quittent l'état Erreur.",
    guarantee_3: "• Tube nommé local restreint par DACL au SID de l'utilisateur courant : GetStatus, Suspend, ReloadConfig et Resume.",
    guarantee_4: "• Aucun texte SendText ni aucune valeur de configuration n'est journalisé ou exposé dans les erreurs.",
    guarantee_5: "• Fermer cette fenêtre laisse le démon actif dans la barre d'état ; Quitter depuis le menu ferme aussi l'UI.",
    footer_invalid_disk: "Fichier sur le disque invalide — ouvrez Paramètres pour restaurer",
    footer_valid: "Configuration valide",
    footer_unsaved: "Modifications non enregistrées",
    btn_discard: "Annuler",
    btn_save: "Enregistrer",
    modal_title_delete: "Confirmer la suppression",
    modal_title_duplicates: "Confirmer la suppression des doublons",
    modal_title_overwrite: "Confirmer l'écrasement",
    modal_title_unsaved: "Modifications non enregistrées",
    modal_exit_unsaved_confirm: "Vous avez des modifications non enregistrées.\nFermer sans enregistrer ?",
    btn_cancel: "Annuler",
    btn_save_short: "Enregistrer",
    capture_edit_chip_tip: "Cliquer pour modifier",
    btn_overwrite: "Écraser",
    btn_switch_file: "Changer de fichier",
    btn_close_without_saving: "Fermer sans enregistrer",
    btn_save_and_close: "Enregistrer et fermer",
    toast_discarded: "Modifications non enregistrées annulées.",
    toast_saved: "Configuration enregistrée en toute sécurité.",
    toast_reloaded_disk: "Configuration rechargée depuis le disque.",
    toast_daemon_reloaded: "Configuration enregistrée et rechargée dans le démon.",
    toast_daemon_not_running_save: "Enregistré sur le disque, mais le démon n'est pas en cours d'exécution.",
    toast_suspended: "Démon suspendu avec succès.",
    toast_suspend_error_state: "Le démon est en erreur : aucune modification appliquée.",
    toast_resumed: "Démon repris avec succès.",
    toast_resume_error_state: "Le démon est en erreur : reprise impossible (fail-closed).",
    toast_autostart_on: "Lancement à l'ouverture de session Windows activé.",
    toast_autostart_off: "Lancement à l'ouverture de session Windows désactivé.",
    toast_fallback_default: "Au moins un groupe doit rester actif : repli sur 'default'.",
    drop_overlay_import: "Déposez le fichier .toml pour importer la configuration ou un thème",
    toast_drop_single_file: "Déposez un seul fichier .toml à la fois.",
    toast_drop_only_toml: "Seuls les fichiers de configuration .toml peuvent être importés.",
    btn_check_updates: "Rechercher des mises à jour",
    btn_checking_updates: "Vérification...",
    section_updates_project: "Mises à jour & Projet",
    checkbox_auto_check_updates: "Rechercher automatiquement les mises à jour",
    btn_changelog: "Changelog",
    btn_github_repo: "Dépôt GitHub",
    btn_close: "Fermer",
    btn_update_now: "Mettre à jour",
    btn_updating: "Mise à jour...",
    modal_update_unsaved_confirm: "Modifications non enregistrées.\nMettre à jour quand même ?",
    btn_update_without_saving: "Mettre à jour sans enregistrer",
    btn_save_and_update: "Enregistrer et mettre à jour",
};

const DE: UiStrings = UiStrings {
    tab_rules: "Regeln",
    tab_settings: "Einstellungen",
    tab_security: "Status & Sicherheit",
    tab_security_alert: "Status & Sicherheit (!)",
    subtab_rule_list: "Regelliste",
    subtab_groups: "Gruppen",
    subtab_duplicates: "Doppelte Regeln",
    subtab_general: "Allgemein",
    subtab_themes: "Designs",
    subtab_shortcuts: "Tastenkürzel",
    section_editor_shortcuts: "Tastenkürzel des Editors",
    shortcuts_intro: "Strg+Z, Strg+Y und Entf greifen nur, wenn kein Textfeld fokussiert ist.",
    shortcut_save: "Konfiguration speichern und im Daemon neu laden",
    shortcut_new_rule: "Neue Regel erstellen",
    shortcut_search: "Regelsuche fokussieren",
    shortcut_undo: "Letzte Änderung rückgängig machen",
    shortcut_redo: "Rückgängig gemachte Änderung wiederherstellen",
    shortcut_delete_rule: "Ausgewählte Regel oder Gruppe löschen (mit Bestätigung)",
    drag_action_tooltip: "Zum Sortieren ziehen",
    status_loading: "Wird geprüft...",
    status_active: "Aktiv",
    status_paused: "Pausiert",
    status_error: "Fehler",
    status_not_running: "Nicht aktiv",
    status_ipc_error: "IPC-Fehler",
    menu_daemon: "Daemon",
    menu_refresh: "Status aktualisieren",
    menu_suspend: "Daemon pausieren",
    menu_resume: "Daemon fortsetzen",
    menu_reload: "Im Daemon neu laden",
    rules_blocked_invalid: "Bearbeitung gesperrt: config.toml auf der Festplatte ist ungültig (Fail-Closed).",
    go_to_settings_restore: "Einstellungen zum Sichern und Wiederherstellen öffnen",
    btn_new: "Erstellen",
    btn_duplicate: "Duplizieren",
    label_groups: "Gruppen:",
    label_search: "Suche:",
    filter_all_groups: "Alle Gruppen",
    filter_no_group: "Ohne Gruppe",
    search_hint_rules: "Name, Gruppe, Hotkey",
    btn_clear: "Leeren",
    filter_only_active: "Nur aktive",
    filter_only_disabled: "Nur deaktivierte",
    empty_rules_filtered: "Keine Regeln gefunden.",
    no_group_paren: "(keine Gruppe)",
    checkbox_enabled: "Aktiv",
    disabled_no_group_tip: "Weisen Sie zuerst mindestens eine Gruppe zu",
    btn_up: "Hoch",
    btn_down: "Runter",
    btn_delete: "Löschen",
    select_or_create_rule: "Wählen Sie links eine Regel oder klicken Sie auf „Erstellen“.",
    rule_detail_title: "Regeldetails",
    label_name: "Name:",
    btn_new_group: "Erstellen",
    label_trigger: "Auslöser:",
    btn_record_trigger: "Aufnehmen",
    btn_record_actions: "Aufnehmen",
    checkbox_wait_release: "Vor Ausführung auf Loslassen der Auslösertasten warten",
    label_context: "Kontext:",
    context_mode_global: "Überall (global)",
    rule_context_inherit: "Überall / von Gruppen erben",
    warning_empty_context_intersection: "Hinweis: Dieser Filter überschneidet sich nie mit dem Gruppenkontext (Regel bleibt inaktiv).",
    context_mode_include: "Nur in ausgewählten Apps",
    context_mode_exclude: "Überall außer in",
    label_process_exe: "Prozess (.exe):",
    process_exe_hint: "z. B. code.exe, chrome.exe",
    btn_choose_app: "Apps...",
    btn_browse_exe: ".exe wählen...",
    search_apps_hint: "Anwendungen filtern...",
    btn_refresh_apps: "Aktualisieren",
    no_apps_found: "Keine Anwendungen gefunden",
    modal_record_trigger_title: "Auslöser aufnehmen",
    modal_record_actions_title: "Aktionen aufnehmen",
    record_trigger_hint: "Drücken Sie die gewünschte Tastenkombination (inkl. Esc, Tab, Leertaste, Pfeile). Falls eine aktive Regel sie abfängt, pausieren Sie den Daemon zuerst.",
    record_actions_hint: "Drücken Sie Tasten oder nutzen Sie Mausklicks/Mausrad im Feld unten. Drücken+Loslassen wird zu Click; gehaltene Eingaben bleiben Down/Up.",
    record_waiting_keys: "Warte auf Tastendruck...",
    record_waiting_actions: "Warte auf Tasten, Mausklicks oder Mausrad in diesem Feld...",
    checkbox_record_mouse_move: "Relative Mausbewegungen im Feld erfassen",
    record_current_modifiers: "Gehaltene Zusatztasten:",
    record_captured_trigger: "Erfasster Auslöser:",
    record_captured_actions: "Aufgenommene Aktionen:",
    btn_reset: "Zurücksetzen",
    btn_confirm: "Bestätigen",
    groups_title: "Gruppen",
    label_new: "Name:",
    group_hint: "z. B. arbeit_1",
    btn_create: "Erstellen",
    filter_groups_hint: "Gruppen filtern",
    empty_groups: "Keine Gruppen gefunden.",
    default_suffix: "Standard",
    checkbox_group_active: "Aktiv",
    all_groups_active_note: "(Modus: alle Gruppen aktiv)",
    available_rules_title: "Verfügbare Regeln",
    group_members_title: "Regeln in Gruppe",
    btn_add_to_group: "Hinzufügen",
    btn_remove_from_group: "Entfernen",
    btn_select_all: "Alle auswählen",
    btn_deselect_all: "Auswahl aufheben",
    search_hint_name_hotkey: "Name oder Hotkey",
    empty_rules_configured: "Keine Regeln konfiguriert.",
    already_in_group_tip: "Regel gehört bereits zu dieser Gruppe",
    empty_group_members: "Keine Regeln in dieser Gruppe.",
    btn_open: "Öffnen",
    dup_col_keep: "Zu behaltende Regel",
    dup_col_status: "Status",
    dup_col_trigger: "Auslöser",
    dup_col_groups: "Gruppen",
    status_word_active: "Aktiv",
    status_word_inactive: "Deaktiviert",
    dup_merge_checkbox: "Zu löschende Regeln in ihren Gruppen durch die behaltene Regel ersetzen",
    settings_blocked_invalid: "Einstellungen gesperrt: Stellen Sie zuerst config.toml im Abschnitt Konfigurationsdatei unten wieder her.",
    section_input_controls: "Tastenkürzel & Eingabelimits",
    label_kill_switch: "Kill-Switch:",
    btn_restore_default: "Standard",
    label_resume_daemon: "Daemon fortsetzen:",
    checkbox_enable_hotkey: "Hotkey aktivieren",
    label_rate_limit: "Limit Ereignisse/Sek.:",
    section_startup_groups: "Startgruppen",
    checkbox_all_groups_enabled: "Alle Gruppen aktiviert",
    startup_groups_note: "Wenn alle spezifischen Gruppen deaktiviert werden, bleibt 'default' aktiv. Neue Gruppen unter Regeln -> Gruppen erstellen.",
    section_autostart: "Mit Windows starten",
    checkbox_autostart: "RustBind bei Windows-Anmeldung starten",
    autostart_note: "Registriert den Daemon unter HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run ohne Adminrechte.",
    section_language: "Sprache",
    label_ui_language: "Oberflächensprache:",
    btn_import_config: "Konfiguration importieren...",
    btn_export_config: "Konfiguration exportieren...",
    select_active_config_hint: "Aktive Konfigurationsdatei auswählen (nur eine gleichzeitig):",
    checkbox_true_black: "True Black (reines Schwarz #000000 für dunkle Designs)",
    true_black_light_tip: "Wählen Sie ein dunkles Design für True Black",
    section_dark_themes: "Dunkle Designs",
    section_light_themes: "Helle Designs",
    section_custom_theme: "Benutzerdefiniertes Design",
    custom_theme_card_title: "Benutzerdefiniertes Design",
    btn_edit_theme: "Bearbeiten",
    btn_import_theme: "Design importieren...",
    btn_export_theme: "Design exportieren...",
    btn_back: "Zurück",
    btn_reset_custom_theme: "Zurücksetzen",
    theme_editor_title: "Design-Editor",
    theme_cat_mode: "Modus & Basisoptionen",
    theme_cat_colors: "Farbschema",
    theme_cat_radii: "Eckenradien",
    theme_mode_label: "Basismodus:",
    theme_mode_dark: "Dunkel",
    theme_mode_light: "Hell",
    color_bg_panel: "Fenster- / Panel-Hintergrund",
    color_bg_card: "Karten- & Popup-Hintergrund",
    color_bg_input: "Eingabefeld-Hintergrund",
    color_bg_widget: "Schaltflächen-Hintergrund",
    color_accent: "Primärer Akzent",
    color_secondary_accent: "Sekundärer Akzent",
    color_text: "Basistext",
    color_muted_text: "Sekundärtext",
    color_border: "Rahmen & Trennlinien",
    radius_controls: "Schaltflächen, Eingaben & Tasten:",
    radius_inner_boxes: "Innenrahmen (z. B. Duplikat-Aktionen):",
    radius_cards_menus: "Karten, Hinweise, Menüs & Farbwähler:",
    radius_modals: "Dialoge & schwebende Fenster:",
    toast_theme_imported: "Benutzerdefiniertes Design importiert und aktiviert.",
    toast_theme_exported: "Benutzerdefiniertes Design exportiert.",
    section_daemon_status: "Daemon-Verbindungsstatus",
    section_daemon_conflicts: "Registrierungskonflikte im Daemon",
    no_daemon_conflicts: "Keine Registrierungskonflikte vom Daemon gemeldet.",
    daemon_resume_hotkey_failed: "Fortsetzen-Hotkey konnte in Windows nicht registriert werden (belegt).",
    btn_open_rule: "Regel öffnen",
    badge_daemon_skipped: "Vom Daemon übersprungen",
    badge_trigger_conflict: "Auslöser-Konflikt",
    badge_trigger_shared: "Auslöser geteilt",
    conflict_kill_switch: "Entspricht dem Kill-Switch: Diese Regel wird vom Daemon ignoriert.",
    conflict_resume_hotkey: "Entspricht dem Fortsetzen-Hotkey: Diese Regel wird ignoriert.",
    stat_total_rules: "Registrierte Regeln:",
    stat_enabled_rules: "Aktive Regeln:",
    stat_total_groups: "Registrierte Gruppen:",
    stat_active_groups: "Aktive Gruppen:",
    stat_active_rules_in_active_groups: "Aktive Regeln in aktiven Gruppen:",
    daemon_menu_hint: "Verwenden Sie das Menü „Daemon“ oben rechts zum Aktualisieren, Pausieren/Fortsetzen oder Neuladen.",
    section_config_file: "Konfigurationsdatei",
    btn_reload_from_disk: "Von Festplatte laden",
    btn_open_config_folder: "Config-Ordner öffnen",
    btn_open_logs_folder: "Log-Ordner öffnen",
    section_recent_log_warnings: "Letzte Warnungen / Fehler im Daemon-Log",
    btn_refresh_logs: "Logs aktualisieren",
    no_recent_log_warnings: "Keine aktuellen Warnungen oder Fehler in rustbind.log.",
    btn_undo: "Rückgängig",
    btn_redo: "Wiederholen",
    label_path_prefix: "Pfad",
    missing_file_note: "Datei fehlt: Entwurf im Speicher (nichts geschrieben bis zum Speichern).",
    invalid_file_title: "Ungültige Konfigurationsdatei (Fail-Closed-Modus)",
    invalid_file_untouched: "Die Originaldatei auf der Festplatte wurde weder geändert noch verschoben oder überschrieben.",
    checkbox_confirm_backup: "Sicherungskopie erstellen und leere Standardkonfiguration wiederherstellen",
    btn_backup_restore: "Sicherung erstellen & wiederherstellen",
    section_guarantees: "Sicherheits-, Datenschutz- & IPC-Garantien (v1)",
    guarantee_1: "• Nur offizielle Windows-APIs (RegisterHotKey, SendInput, Shell_NotifyIconW); kein WH_KEYBOARD_LL-Hook und kein LaunchProcess.",
    guarantee_2: "• Striktes Fail-Closed: Bei ungültiger config.toml startet keine Regel; weder IPC-Resume noch Hotkey verlassen den Fehlerzustand.",
    guarantee_3: "• Lokale Named Pipe per DACL auf die aktuelle Benutzer-SID beschränkt: nur GetStatus, Suspend, ReloadConfig, Resume.",
    guarantee_4: "• SendText-Inhalte und Konfigurationswerte werden niemals protokolliert oder in Fehlern angezeigt.",
    guarantee_5: "• Beim Schließen dieses Fensters bleibt der Daemon im Tray aktiv; Beenden im Tray schließt auch die UI.",
    footer_invalid_disk: "Datei auf Festplatte ungültig — in den Einstellungen wiederherstellen",
    footer_valid: "Konfiguration gültig",
    footer_unsaved: "Ungespeicherte Änderungen",
    btn_discard: "Verwerfen",
    btn_save: "Konfiguration speichern",
    modal_title_delete: "Löschen bestätigen",
    modal_title_duplicates: "Duplikate löschen",
    modal_title_overwrite: "Überschreiben bestätigen",
    modal_title_unsaved: "Ungespeicherte Änderungen",
    modal_exit_unsaved_confirm: "Es gibt ungespeicherte Änderungen.\nOhne Speichern schließen?",
    btn_cancel: "Abbrechen",
    btn_save_short: "Speichern",
    capture_edit_chip_tip: "Zum Bearbeiten klicken",
    btn_overwrite: "Überschreiben",
    btn_switch_file: "Datei wechseln",
    btn_close_without_saving: "Ohne Speichern schließen",
    btn_save_and_close: "Speichern und schließen",
    toast_discarded: "Ungespeicherte Änderungen verworfen.",
    toast_saved: "Konfiguration sicher gespeichert.",
    toast_reloaded_disk: "Konfiguration neu von Festplatte geladen.",
    toast_daemon_reloaded: "Konfiguration gespeichert und im Daemon neu geladen.",
    toast_daemon_not_running_save: "Gespeichert, aber Daemon läuft nicht für Live-Neuladen.",
    toast_suspended: "Daemon erfolgreich pausiert.",
    toast_suspend_error_state: "Daemon ist im Fehlerzustand: keine Änderung.",
    toast_resumed: "Daemon erfolgreich fortgesetzt.",
    toast_resume_error_state: "Daemon ist im Fehlerzustand: Fortsetzen gesperrt (Fail-Closed).",
    toast_autostart_on: "Autostart bei Windows-Anmeldung aktiviert.",
    toast_autostart_off: "Autostart bei Windows-Anmeldung deaktiviert.",
    toast_fallback_default: "Mindestens eine Gruppe muss aktiv bleiben: Rückfall auf 'default'.",
    drop_overlay_import: ".toml-Datei zum Importieren (Konfiguration oder Design) ablegen",
    toast_drop_single_file: "Ziehen Sie jeweils nur eine einzelne .toml-Datei.",
    toast_drop_only_toml: "Es können nur .toml-Konfigurationsdateien importiert werden.",
    btn_check_updates: "Nach Updates suchen",
    btn_checking_updates: "Wird geprüft...",
    section_updates_project: "Updates & Projekt",
    checkbox_auto_check_updates: "Automatisch nach Updates suchen",
    btn_changelog: "Changelog",
    btn_github_repo: "GitHub-Repository",
    btn_close: "Schließen",
    btn_update_now: "Aktualisieren",
    btn_updating: "Wird aktualisiert...",
    modal_update_unsaved_confirm: "Es gibt ungespeicherte Änderungen.\nTrotzdem aktualisieren?",
    btn_update_without_saving: "Ohne Speichern aktualisieren",
    btn_save_and_update: "Speichern und aktualisieren",
};

const ES: UiStrings = UiStrings {
    tab_rules: "Reglas",
    tab_settings: "Ajustes",
    tab_security: "Estado y seguridad",
    tab_security_alert: "Estado y seguridad (!)",
    subtab_rule_list: "Lista de reglas",
    subtab_groups: "Grupos",
    subtab_duplicates: "Reglas duplicadas",
    subtab_general: "General",
    subtab_themes: "Temas",
    subtab_shortcuts: "Atajos",
    section_editor_shortcuts: "Atajos de teclado del editor",
    shortcuts_intro: "Ctrl+Z, Ctrl+Y y Supr solo actúan cuando no estás escribiendo en un campo de texto.",
    shortcut_save: "Guardar configuración y recargar en el daemon",
    shortcut_new_rule: "Crear una nueva regla",
    shortcut_search: "Enfocar búsqueda de reglas",
    shortcut_undo: "Deshacer el último cambio",
    shortcut_redo: "Rehacer el cambio deshecho",
    shortcut_delete_rule: "Eliminar la regla o el grupo seleccionado (pide confirmación)",
    drag_action_tooltip: "Arrastra para reordenar",
    status_loading: "Comprobando...",
    status_active: "Activo",
    status_paused: "En pausa",
    status_error: "Error",
    status_not_running: "No iniciado",
    status_ipc_error: "Error IPC",
    menu_daemon: "Daemon",
    menu_refresh: "Actualizar estado",
    menu_suspend: "Suspender daemon",
    menu_resume: "Reanudar daemon",
    menu_reload: "Recargar en el daemon",
    rules_blocked_invalid: "Edición bloqueada: config.toml en disco no es válido (fail-closed).",
    go_to_settings_restore: "Ir a Ajustes para copia de seguridad y restaurar",
    btn_new: "Crear",
    btn_duplicate: "Duplicar",
    label_groups: "Grupos:",
    label_search: "Buscar:",
    filter_all_groups: "Todos los grupos",
    filter_no_group: "Sin grupo",
    search_hint_rules: "nombre, grupo, atajo",
    btn_clear: "Limpiar",
    filter_only_active: "Solo activas",
    filter_only_disabled: "Solo desactivadas",
    empty_rules_filtered: "No se encontraron reglas.",
    no_group_paren: "(sin grupo)",
    checkbox_enabled: "Activa",
    disabled_no_group_tip: "Asigna al menos un grupo antes de activar esta regla",
    btn_up: "Subir",
    btn_down: "Bajar",
    btn_delete: "Eliminar",
    select_or_create_rule: "Selecciona una regla a la izquierda o pulsa \"Crear\".",
    rule_detail_title: "Detalle de la regla",
    label_name: "Nombre:",
    btn_new_group: "Crear",
    label_trigger: "Disparador:",
    btn_record_trigger: "Grabar",
    btn_record_actions: "Grabar",
    checkbox_wait_release: "Esperar a soltar las teclas antes de ejecutar",
    label_context: "Contexto:",
    context_mode_global: "En todas partes (global)",
    rule_context_inherit: "En todas partes / heredar de grupos",
    warning_empty_context_intersection: "Aviso: este filtro nunca coincide con el contexto de los grupos activos (regla inactiva).",
    context_mode_include: "Solo en apps seleccionadas",
    context_mode_exclude: "En todas partes excepto",
    label_process_exe: "Proceso (.exe):",
    process_exe_hint: "ej. code.exe, chrome.exe",
    btn_choose_app: "Apps...",
    btn_browse_exe: "Examinar .exe...",
    search_apps_hint: "Filtrar aplicaciones...",
    btn_refresh_apps: "Actualizar",
    no_apps_found: "No se encontraron aplicaciones",
    modal_record_trigger_title: "Grabar disparador",
    modal_record_actions_title: "Grabar acciones",
    record_trigger_hint: "Pulsa la combinación de teclas para usar como disparador (incluye Esc, Tab, Espacio, flechas). Si una regla activa la intercepta, suspende el daemon primero.",
    record_actions_hint: "Pulsa teclas o usa clics/rueda del ratón dentro del recuadro inferior. Pulsar+soltar crea Click; las entradas mantenidas quedan en Down/Up.",
    record_waiting_keys: "Esperando pulsación de teclas...",
    record_waiting_actions: "Esperando teclas, clics o rueda del ratón en este recuadro...",
    checkbox_record_mouse_move: "Incluir movimientos relativos del ratón",
    record_current_modifiers: "Modificadores pulsados:",
    record_captured_trigger: "Disparador capturado:",
    record_captured_actions: "Acciones grabadas:",
    btn_reset: "Restablecer",
    btn_confirm: "Confirmar",
    groups_title: "Grupos",
    label_new: "Nombre:",
    group_hint: "ej. trabajo_1",
    btn_create: "Crear",
    filter_groups_hint: "filtrar grupos",
    empty_groups: "No se encontraron grupos.",
    default_suffix: "predeterminado",
    checkbox_group_active: "Activo",
    all_groups_active_note: "(modo: todos los grupos activos)",
    available_rules_title: "Reglas disponibles",
    group_members_title: "Reglas en el grupo",
    btn_add_to_group: "Añadir",
    btn_remove_from_group: "Quitar",
    btn_select_all: "Seleccionar todas",
    btn_deselect_all: "Deseleccionar todas",
    search_hint_name_hotkey: "nombre o atajo",
    empty_rules_configured: "No hay reglas configuradas.",
    already_in_group_tip: "La regla ya pertenece a este grupo",
    empty_group_members: "No hay reglas en este grupo.",
    btn_open: "Abrir",
    dup_col_keep: "Regla a conservar",
    dup_col_status: "Estado",
    dup_col_trigger: "Disparador",
    dup_col_groups: "Grupos",
    status_word_active: "Activa",
    status_word_inactive: "Desactivada",
    dup_merge_checkbox: "Sustituir las reglas a eliminar por la regla conservada en los grupos en los que están presentes",
    settings_blocked_invalid: "Ajustes bloqueados: restaura primero config.toml en la sección Archivo de configuración abajo.",
    section_input_controls: "Atajos y límites de entrada",
    label_kill_switch: "Kill-Switch:",
    btn_restore_default: "Restaurar",
    label_resume_daemon: "Reanudar daemon:",
    checkbox_enable_hotkey: "Activar atajo",
    label_rate_limit: "Límite eventos/s:",
    section_startup_groups: "Grupos al inicio",
    checkbox_all_groups_enabled: "Todos los grupos activos",
    startup_groups_note: "Si desactivas todos los grupos específicos, 'default' permanece activo automáticamente. Crea grupos en Reglas -> Grupos.",
    section_autostart: "Inicio con Windows",
    checkbox_autostart: "Iniciar RustBind al iniciar sesión en Windows",
    autostart_note: "Registra el daemon en HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run sin permisos de administrador.",
    section_language: "Idioma",
    label_ui_language: "Idioma de la interfaz:",
    btn_import_config: "Importar configuración...",
    btn_export_config: "Exportar configuración...",
    select_active_config_hint: "Selecciona el archivo de configuración activo (solo uno a la vez):",
    checkbox_true_black: "True Black (negros puros #000000 para temas oscuros)",
    true_black_light_tip: "Selecciona un tema oscuro para activar True Black",
    section_dark_themes: "Temas oscuros",
    section_light_themes: "Temas claros",
    section_custom_theme: "Tema personalizado",
    custom_theme_card_title: "Tema personalizado",
    btn_edit_theme: "Editar",
    btn_import_theme: "Importar tema...",
    btn_export_theme: "Exportar tema...",
    btn_back: "Atrás",
    btn_reset_custom_theme: "Restablecer",
    theme_editor_title: "Editor de tema",
    theme_cat_mode: "Modo y opciones base",
    theme_cat_colors: "Esquema de colores",
    theme_cat_radii: "Radio de bordes",
    theme_mode_label: "Modo base:",
    theme_mode_dark: "Oscuro",
    theme_mode_light: "Claro",
    color_bg_panel: "Fondo de ventana / panel",
    color_bg_card: "Fondo de tarjetas y popups",
    color_bg_input: "Fondo de campos de texto",
    color_bg_widget: "Fondo de botones y controles",
    color_accent: "Acento primario",
    color_secondary_accent: "Acento secundario",
    color_text: "Texto base",
    color_muted_text: "Texto secundario",
    color_border: "Bordes y separadores",
    radius_controls: "Botones, entradas, selectores y teclas:",
    radius_inner_boxes: "Cajas internas (ej. acciones duplicadas):",
    radius_cards_menus: "Tarjetas, avisos, menús y selector:",
    radius_modals: "Ventanas modales y flotantes:",
    toast_theme_imported: "Tema personalizado importado y activado.",
    toast_theme_exported: "Tema personalizado exportado.",
    section_daemon_status: "Estado de conexión del daemon",
    section_daemon_conflicts: "Conflictos de registro en el daemon",
    no_daemon_conflicts: "El daemon no informó conflictos de registro.",
    daemon_resume_hotkey_failed: "No se pudo registrar el atajo para reanudar en Windows (ya ocupado).",
    btn_open_rule: "Abrir regla",
    badge_daemon_skipped: "Omitida por el daemon",
    badge_trigger_conflict: "Conflicto de atajo",
    badge_trigger_shared: "Atajo compartido",
    conflict_kill_switch: "Coincide con el Kill-Switch: el daemon ignorará esta regla.",
    conflict_resume_hotkey: "Coincide con el atajo para reanudar: esta regla será ignorada.",
    stat_total_rules: "Reglas registradas:",
    stat_enabled_rules: "Reglas activas:",
    stat_total_groups: "Grupos registrados:",
    stat_active_groups: "Grupos activos:",
    stat_active_rules_in_active_groups: "Reglas activas en grupos activos:",
    daemon_menu_hint: "Usa el menú \"Daemon\" en la barra superior para actualizar, suspender/reanudar o recargar.",
    section_config_file: "Archivo de configuración",
    btn_reload_from_disk: "Recargar del disco",
    btn_open_config_folder: "Abrir carpeta config",
    btn_open_logs_folder: "Abrir carpeta logs",
    section_recent_log_warnings: "Últimas advertencias / errores del daemon",
    btn_refresh_logs: "Actualizar logs",
    no_recent_log_warnings: "Sin advertencias ni errores recientes en rustbind.log.",
    btn_undo: "Deshacer",
    btn_redo: "Rehacer",
    label_path_prefix: "Ruta",
    missing_file_note: "Archivo ausente: borrador en memoria (no se escribe nada hasta guardar).",
    invalid_file_title: "Archivo de configuración no válido (modo fail-closed)",
    invalid_file_untouched: "El archivo original en disco no se ha modificado, movido ni sobrescrito.",
    checkbox_confirm_backup: "Confirmo crear una copia de seguridad y restaurar una configuración vacía",
    btn_backup_restore: "Crear copia y restaurar",
    section_guarantees: "Garantías de seguridad, privacidad e IPC (v1)",
    guarantee_1: "• Solo API oficiales de Windows (RegisterHotKey, SendInput, Shell_NotifyIconW); sin hooks WH_KEYBOARD_LL ni LaunchProcess.",
    guarantee_2: "• Fail-closed estricto: si config.toml no es válido, ninguna regla se ejecuta ni se puede salir de Error vía IPC/atajo.",
    guarantee_3: "• Named Pipe local restringido por DACL al SID del usuario actual: GetStatus, Suspend, ReloadConfig y Resume.",
    guarantee_4: "• Ningún texto SendText ni valor de configuración se registra o expone en los errores.",
    guarantee_5: "• Cerrar esta ventana mantiene el daemon en la bandeja; Salir en la bandeja también cierra la UI.",
    footer_invalid_disk: "Archivo en disco no válido — abre Ajustes para restaurar",
    footer_valid: "Configuración válida",
    footer_unsaved: "Cambios sin guardar",
    btn_discard: "Descartar cambios",
    btn_save: "Guardar configuración",
    modal_title_delete: "Confirmar eliminación",
    modal_title_duplicates: "Confirmar eliminación de duplicados",
    modal_title_overwrite: "Confirmar sobrescritura",
    modal_title_unsaved: "Cambios sin guardar",
    modal_exit_unsaved_confirm: "Hay cambios sin guardar.\n¿Cerrar sin guardar?",
    btn_cancel: "Cancelar",
    btn_save_short: "Guardar",
    capture_edit_chip_tip: "Haz clic para editar",
    btn_overwrite: "Sobrescribir",
    btn_switch_file: "Cambiar archivo",
    btn_close_without_saving: "Cerrar sin guardar",
    btn_save_and_close: "Guardar y cerrar",
    toast_discarded: "Cambios no guardados descartados.",
    toast_saved: "Configuración guardada de forma segura.",
    toast_reloaded_disk: "Configuración recargada desde el disco.",
    toast_daemon_reloaded: "Configuración guardada y recargada en el daemon.",
    toast_daemon_not_running_save: "Guardado en disco, pero el daemon no está en ejecución.",
    toast_suspended: "Daemon suspendido correctamente.",
    toast_suspend_error_state: "El daemon está en Error: no se aplicaron cambios.",
    toast_resumed: "Daemon reanudado correctamente.",
    toast_resume_error_state: "El daemon está en Error: imposible reanudar (fail-closed).",
    toast_autostart_on: "Inicio automático con Windows activado.",
    toast_autostart_off: "Inicio automático con Windows desactivado.",
    toast_fallback_default: "Al menos un grupo debe seguir activo: retorno a 'default'.",
    drop_overlay_import: "Suelta el archivo .toml para importar configuración o tema",
    toast_drop_single_file: "Arrastra un solo archivo .toml a la vez.",
    toast_drop_only_toml: "Solo se pueden importar archivos de configuración .toml.",
    btn_check_updates: "Buscar actualizaciones",
    btn_checking_updates: "Comprobando...",
    section_updates_project: "Actualizaciones y Proyecto",
    checkbox_auto_check_updates: "Buscar actualizaciones automáticamente",
    btn_changelog: "Changelog",
    btn_github_repo: "Repositorio de GitHub",
    btn_close: "Cerrar",
    btn_update_now: "Actualizar",
    btn_updating: "Actualizando...",
    modal_update_unsaved_confirm: "Hay cambios sin guardar.\n¿Actualizar de todos modos?",
    btn_update_without_saving: "Actualizar sin guardar",
    btn_save_and_update: "Guardar y actualizar",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_five_languages_supported_with_english_default() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(Language::ALL.len(), 5);
        for lang in Language::ALL {
            assert_eq!(Language::from_code(lang.code()), Some(*lang));
            assert!(!lang.s().tab_rules.is_empty());
            assert!(!lang.theme_description(ThemeId::GtaViViceSunset).is_empty());
        }
    }
}
