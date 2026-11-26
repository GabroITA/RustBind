use std::fs;
use std::path::Path;

use eframe::egui;

use crate::i18n::Language;

/// Identificatore univoco dei temi visivi (10 predefiniti + tema personalizzato).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeId {
    #[default]
    RustBindClassic,
    GtaViViceSunset,
    Dracula,
    NordNight,
    TokyoNight,
    CatppuccinLatte,
    MacOsLight,
    RosePineDawn,
    SolarizedLight,
    GruvboxLight,
    Custom,
}

impl ThemeId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RustBindClassic => "rustbind_classic",
            Self::GtaViViceSunset => "gta_vi_vice_sunset",
            Self::Dracula => "dracula",
            Self::NordNight => "nord_night",
            Self::TokyoNight => "tokyo_night",
            Self::CatppuccinLatte => "catppuccin_latte",
            Self::MacOsLight => "macos_light",
            Self::RosePineDawn => "rose_pine_dawn",
            Self::SolarizedLight => "solarized_light",
            Self::GruvboxLight => "gruvbox_light",
            Self::Custom => "custom",
        }
    }

    pub fn from_str(raw: &str) -> Option<Self> {
        match raw.trim() {
            "rustbind_classic" => Some(Self::RustBindClassic),
            "gta_vi_vice_sunset" => Some(Self::GtaViViceSunset),
            "dracula" => Some(Self::Dracula),
            "nord_night" => Some(Self::NordNight),
            "tokyo_night" => Some(Self::TokyoNight),
            "catppuccin_latte" => Some(Self::CatppuccinLatte),
            "macos_light" => Some(Self::MacOsLight),
            "rose_pine_dawn" => Some(Self::RosePineDawn),
            "solarized_light" => Some(Self::SolarizedLight),
            "gruvbox_light" => Some(Self::GruvboxLight),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    pub fn preset(self) -> &'static ThemePreset {
        ALL_THEMES
            .iter()
            .find(|p| p.id == self)
            .unwrap_or(&ALL_THEMES[0])
    }
}

pub const MAX_CORNER_RADIUS: u8 = 24;

/// Raggi di curvatura raggruppati per i valori usati dai componenti con bordo della UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeBorderRadii {
    /// Pulsanti, input di testo, checkbox, combobox e chip keycap (default `6`).
    pub controls: u8,
    /// Box interni secondari come azioni duplicate (default `8`).
    pub inner_boxes: u8,
    /// Card, toast di notifica, tendine select, color picker e menu popup (default `10`).
    pub cards_menus: u8,
    /// Modali e finestre flottanti (default `14`).
    pub modals: u8,
}

impl Default for ThemeBorderRadii {
    fn default() -> Self {
        Self {
            controls: 6,
            inner_boxes: 8,
            cards_menus: 10,
            modals: 14,
        }
    }
}

impl ThemeBorderRadii {
    pub fn from_card_radius(card_radius: u8) -> Self {
        Self {
            controls: 6,
            inner_boxes: card_radius.min(8),
            cards_menus: card_radius,
            modals: 14,
        }
    }

    fn clamped(self) -> Self {
        Self {
            controls: self.controls.min(MAX_CORNER_RADIUS),
            inner_boxes: self.inner_boxes.min(MAX_CORNER_RADIUS),
            cards_menus: self.cards_menus.min(MAX_CORNER_RADIUS),
            modals: self.modals.min(MAX_CORNER_RADIUS),
        }
    }
}

/// Definizione completa del singolo tema personalizzabile dall'utente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomTheme {
    pub is_dark: bool,
    pub true_black: bool,
    pub bg_panel: egui::Color32,
    pub bg_card: egui::Color32,
    pub bg_input: egui::Color32,
    pub bg_widget: egui::Color32,
    pub accent: egui::Color32,
    pub secondary_accent: egui::Color32,
    pub text: egui::Color32,
    pub muted_text: egui::Color32,
    pub border: egui::Color32,
    pub radii: ThemeBorderRadii,
}

impl Default for CustomTheme {
    fn default() -> Self {
        Self::from_preset(&ALL_THEMES[0], false)
    }
}

impl CustomTheme {
    pub fn from_preset(preset: &ThemePreset, true_black: bool) -> Self {
        Self {
            is_dark: preset.is_dark,
            true_black: preset.is_dark && true_black,
            bg_panel: preset.bg_panel,
            bg_card: preset.bg_card,
            bg_input: preset.bg_input,
            bg_widget: preset.bg_widget,
            accent: preset.accent,
            secondary_accent: preset.secondary_accent,
            text: preset.text,
            muted_text: preset.muted_text,
            border: preset.border,
            radii: ThemeBorderRadii::default(),
        }
    }

    pub fn to_toml_table(self) -> toml::map::Map<String, toml::Value> {
        let mut root = toml::map::Map::new();
        root.insert("schema_version".to_string(), toml::Value::Integer(1));
        root.insert("is_dark".to_string(), toml::Value::Boolean(self.is_dark));
        root.insert(
            "true_black".to_string(),
            toml::Value::Boolean(self.true_black),
        );

        let mut colors = toml::map::Map::new();
        colors.insert(
            "bg_panel".to_string(),
            toml::Value::String(format_hex_rgb(self.bg_panel)),
        );
        colors.insert(
            "bg_card".to_string(),
            toml::Value::String(format_hex_rgb(self.bg_card)),
        );
        colors.insert(
            "bg_input".to_string(),
            toml::Value::String(format_hex_rgb(self.bg_input)),
        );
        colors.insert(
            "bg_widget".to_string(),
            toml::Value::String(format_hex_rgb(self.bg_widget)),
        );
        colors.insert(
            "accent".to_string(),
            toml::Value::String(format_hex_rgb(self.accent)),
        );
        colors.insert(
            "secondary_accent".to_string(),
            toml::Value::String(format_hex_rgb(self.secondary_accent)),
        );
        colors.insert(
            "text".to_string(),
            toml::Value::String(format_hex_rgb(self.text)),
        );
        colors.insert(
            "muted_text".to_string(),
            toml::Value::String(format_hex_rgb(self.muted_text)),
        );
        colors.insert(
            "border".to_string(),
            toml::Value::String(format_hex_rgb(self.border)),
        );
        root.insert("colors".to_string(), toml::Value::Table(colors));

        let radii = self.radii.clamped();
        let mut border_radius = toml::map::Map::new();
        border_radius.insert(
            "controls".to_string(),
            toml::Value::Integer(i64::from(radii.controls)),
        );
        border_radius.insert(
            "inner_boxes".to_string(),
            toml::Value::Integer(i64::from(radii.inner_boxes)),
        );
        border_radius.insert(
            "cards_menus".to_string(),
            toml::Value::Integer(i64::from(radii.cards_menus)),
        );
        border_radius.insert(
            "modals".to_string(),
            toml::Value::Integer(i64::from(radii.modals)),
        );
        root.insert(
            "border_radius".to_string(),
            toml::Value::Table(border_radius),
        );

        root
    }

    /// Serializza il tema custom in formato TOML esportabile.
    pub fn to_toml_string(self) -> Result<String, String> {
        toml::to_string_pretty(&toml::Value::Table(self.to_toml_table())).map_err(|e| e.to_string())
    }

    /// Legge una tabella TOML tollerando eventuali campi mancanti (per `preferences.toml`).
    pub fn from_toml_table_lenient(table: &toml::map::Map<String, toml::Value>) -> Self {
        let mut theme = Self::default();
        if let Some(is_dark) = table.get("is_dark").and_then(toml::Value::as_bool) {
            theme.is_dark = is_dark;
        }
        if let Some(tb) = table.get("true_black").and_then(toml::Value::as_bool) {
            theme.true_black = tb;
        }
        if let Some(colors) = table.get("colors").and_then(toml::Value::as_table) {
            apply_color_field(colors, "bg_panel", &mut theme.bg_panel);
            apply_color_field(colors, "bg_card", &mut theme.bg_card);
            apply_color_field(colors, "bg_input", &mut theme.bg_input);
            apply_color_field(colors, "bg_widget", &mut theme.bg_widget);
            apply_color_field(colors, "accent", &mut theme.accent);
            apply_color_field(colors, "secondary_accent", &mut theme.secondary_accent);
            apply_color_field(colors, "text", &mut theme.text);
            apply_color_field(colors, "muted_text", &mut theme.muted_text);
            apply_color_field(colors, "border", &mut theme.border);
        }
        if let Some(radii) = table.get("border_radius").and_then(toml::Value::as_table) {
            apply_radius_field(radii, "controls", &mut theme.radii.controls);
            apply_radius_field(radii, "inner_boxes", &mut theme.radii.inner_boxes);
            apply_radius_field(radii, "cards_menus", &mut theme.radii.cards_menus);
            apply_radius_field(radii, "modals", &mut theme.radii.modals);
        }
        theme.radii = theme.radii.clamped();
        theme
    }

    /// Valida ed importa un tema custom da un file TOML dedicato.
    pub fn from_toml_str(raw: &str) -> Result<Self, String> {
        let doc: toml::Value = toml::from_str(raw).map_err(|e| e.to_string())?;
        let table = doc
            .as_table()
            .ok_or_else(|| "Root TOML must be a table".to_string())?;
        if let Some(version) = table.get("schema_version")
            && version.as_integer() != Some(1)
        {
            return Err("Unsupported custom theme schema_version".to_string());
        }
        let is_dark = table
            .get("is_dark")
            .and_then(toml::Value::as_bool)
            .ok_or_else(|| "Missing or invalid 'is_dark' boolean".to_string())?;
        let true_black = table
            .get("true_black")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);
        let colors = table
            .get("colors")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| "Missing '[colors]' table".to_string())?;
        let mut theme = Self {
            is_dark,
            true_black,
            ..Self::default()
        };
        theme.bg_panel = parse_required_color(colors, "bg_panel")?;
        theme.bg_card = parse_required_color(colors, "bg_card")?;
        theme.bg_input = parse_required_color(colors, "bg_input")?;
        theme.bg_widget = parse_required_color(colors, "bg_widget")?;
        theme.accent = parse_required_color(colors, "accent")?;
        theme.secondary_accent = parse_required_color(colors, "secondary_accent")?;
        theme.text = parse_required_color(colors, "text")?;
        theme.muted_text = parse_required_color(colors, "muted_text")?;
        theme.border = parse_required_color(colors, "border")?;

        if let Some(radii_val) = table.get("border_radius") {
            let radii = radii_val
                .as_table()
                .ok_or_else(|| "Invalid '[border_radius]' table".to_string())?;
            theme.radii.controls = parse_optional_radius(radii, "controls", theme.radii.controls)?;
            theme.radii.inner_boxes =
                parse_optional_radius(radii, "inner_boxes", theme.radii.inner_boxes)?;
            theme.radii.cards_menus =
                parse_optional_radius(radii, "cards_menus", theme.radii.cards_menus)?;
            theme.radii.modals = parse_optional_radius(radii, "modals", theme.radii.modals)?;
        }
        theme.radii = theme.radii.clamped();
        Ok(theme)
    }
}

fn format_hex_rgb(color: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

fn parse_hex_rgb(raw: &str) -> Option<egui::Color32> {
    let hex = raw.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

fn apply_color_field(
    colors: &toml::map::Map<String, toml::Value>,
    key: &str,
    dest: &mut egui::Color32,
) {
    if let Some(c) = colors
        .get(key)
        .and_then(toml::Value::as_str)
        .and_then(parse_hex_rgb)
    {
        *dest = c;
    }
}

fn apply_radius_field(radii: &toml::map::Map<String, toml::Value>, key: &str, dest: &mut u8) {
    if let Some(v) = radii.get(key).and_then(toml::Value::as_integer)
        && (0..=i64::from(MAX_CORNER_RADIUS)).contains(&v)
    {
        *dest = v as u8;
    }
}

fn parse_required_color(
    colors: &toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<egui::Color32, String> {
    let raw = colors
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("Missing color '{key}'"))?;
    parse_hex_rgb(raw).ok_or_else(|| format!("Invalid hex color for '{key}' (expected #RRGGBB)"))
}

fn parse_optional_radius(
    radii: &toml::map::Map<String, toml::Value>,
    key: &str,
    default: u8,
) -> Result<u8, String> {
    match radii.get(key) {
        None => Ok(default),
        Some(v) => {
            let n = v
                .as_integer()
                .ok_or_else(|| format!("Invalid integer for radius '{key}'"))?;
            if !(0..=i64::from(MAX_CORNER_RADIUS)).contains(&n) {
                return Err(format!(
                    "Radius '{key}' out of range (0..={MAX_CORNER_RADIUS})"
                ));
            }
            Ok(n as u8)
        }
    }
}

/// Definizione cromatica completa di un tema della UI.
#[derive(Debug, Clone, Copy)]
pub struct ThemePreset {
    pub id: ThemeId,
    pub name: &'static str,
    pub is_dark: bool,
    pub bg_panel: egui::Color32,
    pub bg_card: egui::Color32,
    pub bg_input: egui::Color32,
    pub bg_widget: egui::Color32,
    pub accent: egui::Color32,
    pub secondary_accent: egui::Color32,
    pub text: egui::Color32,
    pub muted_text: egui::Color32,
    pub border: egui::Color32,
}

/// Elenco completo dei 10 temi ispirati (5 scuri, 5 chiari).
pub const ALL_THEMES: &[ThemePreset] = &[
    ThemePreset {
        id: ThemeId::RustBindClassic,
        name: "RustBind Classico",
        is_dark: true,
        bg_panel: egui::Color32::from_rgb(0x1B, 0x1B, 0x1E),
        bg_card: egui::Color32::from_rgb(0x24, 0x24, 0x29),
        bg_input: egui::Color32::from_rgb(0x13, 0x13, 0x16),
        bg_widget: egui::Color32::from_rgb(0x2E, 0x2E, 0x36),
        accent: egui::Color32::from_rgb(0xB7, 0x41, 0x0E),
        secondary_accent: egui::Color32::from_rgb(0xE0, 0x7A, 0x42),
        text: egui::Color32::from_rgb(0xEC, 0xEC, 0xF0),
        muted_text: egui::Color32::from_rgb(0xA0, 0xA4, 0xB0),
        border: egui::Color32::from_rgb(0x3D, 0x3D, 0x47),
    },
    ThemePreset {
        id: ThemeId::GtaViViceSunset,
        name: "GTA VI — Vice Sunset",
        is_dark: true,
        bg_panel: egui::Color32::from_rgb(0x12, 0x0B, 0x1E),
        bg_card: egui::Color32::from_rgb(0x1C, 0x12, 0x30),
        bg_input: egui::Color32::from_rgb(0x0B, 0x06, 0x14),
        bg_widget: egui::Color32::from_rgb(0x28, 0x1B, 0x44),
        accent: egui::Color32::from_rgb(0xFF, 0x2E, 0x88),
        secondary_accent: egui::Color32::from_rgb(0x00, 0xE5, 0xFF),
        text: egui::Color32::from_rgb(0xFF, 0xF2, 0xF8),
        muted_text: egui::Color32::from_rgb(0x7B, 0xE3, 0xF2),
        border: egui::Color32::from_rgb(0x4C, 0x2C, 0x72),
    },
    ThemePreset {
        id: ThemeId::Dracula,
        name: "Dracula",
        is_dark: true,
        bg_panel: egui::Color32::from_rgb(0x28, 0x2A, 0x36),
        bg_card: egui::Color32::from_rgb(0x31, 0x34, 0x43),
        bg_input: egui::Color32::from_rgb(0x1E, 0x20, 0x29),
        bg_widget: egui::Color32::from_rgb(0x3D, 0x41, 0x56),
        accent: egui::Color32::from_rgb(0xBD, 0x93, 0xF9),
        secondary_accent: egui::Color32::from_rgb(0xFF, 0x79, 0xC6),
        text: egui::Color32::from_rgb(0xF8, 0xF8, 0xF2),
        muted_text: egui::Color32::from_rgb(0xA5, 0xAF, 0xD2),
        border: egui::Color32::from_rgb(0x4D, 0x52, 0x6D),
    },
    ThemePreset {
        id: ThemeId::NordNight,
        name: "Nord Night",
        is_dark: true,
        bg_panel: egui::Color32::from_rgb(0x2E, 0x34, 0x40),
        bg_card: egui::Color32::from_rgb(0x3B, 0x42, 0x52),
        bg_input: egui::Color32::from_rgb(0x24, 0x29, 0x33),
        bg_widget: egui::Color32::from_rgb(0x43, 0x4C, 0x5E),
        accent: egui::Color32::from_rgb(0x5E, 0x81, 0xAC),
        secondary_accent: egui::Color32::from_rgb(0x88, 0xC0, 0xD0),
        text: egui::Color32::from_rgb(0xEC, 0xEF, 0xF4),
        muted_text: egui::Color32::from_rgb(0xAD, 0xB8, 0xCB),
        border: egui::Color32::from_rgb(0x4C, 0x56, 0x6A),
    },
    ThemePreset {
        id: ThemeId::TokyoNight,
        name: "Tokyo Night",
        is_dark: true,
        bg_panel: egui::Color32::from_rgb(0x1A, 0x1B, 0x26),
        bg_card: egui::Color32::from_rgb(0x24, 0x28, 0x3B),
        bg_input: egui::Color32::from_rgb(0x13, 0x14, 0x1D),
        bg_widget: egui::Color32::from_rgb(0x2F, 0x35, 0x4E),
        accent: egui::Color32::from_rgb(0x7A, 0xA2, 0xF7),
        secondary_accent: egui::Color32::from_rgb(0xBB, 0x9A, 0xF7),
        text: egui::Color32::from_rgb(0xC0, 0xCA, 0xF5),
        muted_text: egui::Color32::from_rgb(0x84, 0x90, 0xBD),
        border: egui::Color32::from_rgb(0x3B, 0x42, 0x61),
    },
    ThemePreset {
        id: ThemeId::CatppuccinLatte,
        name: "Catppuccin Latte",
        is_dark: false,
        bg_panel: egui::Color32::from_rgb(0xEF, 0xF1, 0xF5),
        bg_card: egui::Color32::from_rgb(0xE6, 0xE9, 0xEF),
        bg_input: egui::Color32::from_rgb(0xFF, 0xFF, 0xFF),
        bg_widget: egui::Color32::from_rgb(0xDC, 0xE0, 0xE8),
        accent: egui::Color32::from_rgb(0xFE, 0x64, 0x0B),
        secondary_accent: egui::Color32::from_rgb(0x88, 0x39, 0xEF),
        text: egui::Color32::from_rgb(0x4C, 0x4F, 0x69),
        muted_text: egui::Color32::from_rgb(0x6C, 0x6F, 0x85),
        border: egui::Color32::from_rgb(0xBC, 0xC0, 0xCC),
    },
    ThemePreset {
        id: ThemeId::MacOsLight,
        name: "macOS Light",
        is_dark: false,
        bg_panel: egui::Color32::from_rgb(0xF5, 0xF5, 0xF7),
        bg_card: egui::Color32::from_rgb(0xFF, 0xFF, 0xFF),
        bg_input: egui::Color32::from_rgb(0xFF, 0xFF, 0xFF),
        bg_widget: egui::Color32::from_rgb(0xE8, 0xE8, 0xED),
        accent: egui::Color32::from_rgb(0x00, 0x7A, 0xFF),
        secondary_accent: egui::Color32::from_rgb(0x34, 0xC7, 0x59),
        text: egui::Color32::from_rgb(0x1D, 0x1D, 0x1F),
        muted_text: egui::Color32::from_rgb(0x6E, 0x6E, 0x73),
        border: egui::Color32::from_rgb(0xD2, 0xD2, 0xD7),
    },
    ThemePreset {
        id: ThemeId::RosePineDawn,
        name: "Rosé Pine Dawn",
        is_dark: false,
        bg_panel: egui::Color32::from_rgb(0xFA, 0xF4, 0xED),
        bg_card: egui::Color32::from_rgb(0xFF, 0xFA, 0xF3),
        bg_input: egui::Color32::from_rgb(0xFF, 0xFF, 0xFF),
        bg_widget: egui::Color32::from_rgb(0xF2, 0xE9, 0xE1),
        accent: egui::Color32::from_rgb(0xB4, 0x63, 0x7A),
        secondary_accent: egui::Color32::from_rgb(0x28, 0x69, 0x83),
        text: egui::Color32::from_rgb(0x57, 0x52, 0x79),
        muted_text: egui::Color32::from_rgb(0x79, 0x75, 0x93),
        border: egui::Color32::from_rgb(0xDF, 0xDA, 0xD9),
    },
    ThemePreset {
        id: ThemeId::SolarizedLight,
        name: "Solarized Light",
        is_dark: false,
        bg_panel: egui::Color32::from_rgb(0xFD, 0xF6, 0xE3),
        bg_card: egui::Color32::from_rgb(0xEE, 0xE8, 0xD5),
        bg_input: egui::Color32::from_rgb(0xFF, 0xFD, 0xF5),
        bg_widget: egui::Color32::from_rgb(0xE2, 0xDC, 0xC5),
        accent: egui::Color32::from_rgb(0x26, 0x8B, 0xD2),
        secondary_accent: egui::Color32::from_rgb(0xCB, 0x4B, 0x16),
        text: egui::Color32::from_rgb(0x3C, 0x4B, 0x52),
        muted_text: egui::Color32::from_rgb(0x65, 0x7B, 0x83),
        border: egui::Color32::from_rgb(0xD3, 0xCB, 0xB8),
    },
    ThemePreset {
        id: ThemeId::GruvboxLight,
        name: "Gruvbox Light",
        is_dark: false,
        bg_panel: egui::Color32::from_rgb(0xFB, 0xF1, 0xC7),
        bg_card: egui::Color32::from_rgb(0xF2, 0xE5, 0xBC),
        bg_input: egui::Color32::from_rgb(0xFF, 0xF8, 0xE1),
        bg_widget: egui::Color32::from_rgb(0xEB, 0xDB, 0xB2),
        accent: egui::Color32::from_rgb(0xAF, 0x3A, 0x03),
        secondary_accent: egui::Color32::from_rgb(0x79, 0x74, 0x0E),
        text: egui::Color32::from_rgb(0x3C, 0x38, 0x36),
        muted_text: egui::Color32::from_rgb(0x66, 0x5C, 0x54),
        border: egui::Color32::from_rgb(0xD5, 0xC4, 0xA1),
    },
];

pub const DEFAULT_RULES_SPLIT_RATIO: f32 = 0.36;
pub const DEFAULT_GROUPS_SPLIT_RATIO: f32 = 0.34;
pub const DEFAULT_WINDOW_W: f32 = 900.0;
pub const DEFAULT_WINDOW_H: f32 = 620.0;

fn clamp_split_ratio(v: f32) -> Option<f32> {
    v.is_finite().then(|| v.clamp(0.18, 0.75))
}

fn clamp_window_dim(v: f32, min: f32, max: f32) -> Option<f32> {
    v.is_finite().then(|| v.clamp(min, max))
}

fn clamp_window_pos(v: f32) -> Option<f32> {
    (v.is_finite() && (-5000.0..=10000.0).contains(&v)).then_some(v)
}

/// Preferenze UI persistenti (tema, True Black, tema custom, lingua, split ratio e geometria finestra).
/// Separate da `config.toml` del daemon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeConfig {
    pub theme_id: ThemeId,
    pub true_black: bool,
    pub custom_theme: CustomTheme,
    pub language: Language,
    pub auto_check_updates: bool,
    pub rules_split_ratio: f32,
    pub groups_split_ratio: f32,
    pub window_w: f32,
    pub window_h: f32,
    pub window_x: Option<f32>,
    pub window_y: Option<f32>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            theme_id: ThemeId::default(),
            true_black: false,
            custom_theme: CustomTheme::default(),
            language: Language::default(),
            auto_check_updates: true,
            rules_split_ratio: DEFAULT_RULES_SPLIT_RATIO,
            groups_split_ratio: DEFAULT_GROUPS_SPLIT_RATIO,
            window_w: DEFAULT_WINDOW_W,
            window_h: DEFAULT_WINDOW_H,
            window_x: None,
            window_y: None,
        }
    }
}

impl ThemeConfig {
    /// Restituisce se il tema attualmente attivo utilizza la modalità scura base.
    pub fn effective_is_dark(&self) -> bool {
        match self.theme_id {
            ThemeId::Custom => self.custom_theme.is_dark,
            other => other.preset().is_dark,
        }
    }

    /// Restituisce i raggi effettivi del tema attivo (`Custom` oppure predefiniti).
    pub fn active_radii(&self, card_radius: u8) -> ThemeBorderRadii {
        match self.theme_id {
            ThemeId::Custom => self.custom_theme.radii.clamped(),
            _ => ThemeBorderRadii::from_card_radius(card_radius),
        }
    }
}

/// Nome del file TOML delle preferenze UI nella directory dati di RustBind.
pub const UI_PREFERENCES_FILENAME: &str = "preferences.toml";

fn toml_as_f32(val: Option<&toml::Value>) -> Option<f32> {
    match val? {
        toml::Value::Float(v) => Some(*v as f32),
        toml::Value::Integer(v) => Some(*v as f32),
        _ => None,
    }
}

impl ThemeConfig {
    pub fn load_from_file(path: &Path) -> Self {
        let Ok(content) = fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(doc) = toml::from_str::<toml::Value>(&content) else {
            return Self::default();
        };
        let mut cfg = Self::default();
        if let Some(id) = doc
            .get("theme")
            .and_then(toml::Value::as_str)
            .and_then(ThemeId::from_str)
        {
            cfg.theme_id = id;
        }
        if let Some(tb) = doc.get("true_black").and_then(toml::Value::as_bool) {
            cfg.true_black = tb;
        }
        if let Some(lang) = doc
            .get("language")
            .and_then(toml::Value::as_str)
            .and_then(Language::from_code)
        {
            cfg.language = lang;
        }
        if let Some(auto_check) = doc.get("auto_check_updates").and_then(toml::Value::as_bool) {
            cfg.auto_check_updates = auto_check;
        }
        if let Some(clamped) = toml_as_f32(doc.get("rules_split_ratio")).and_then(clamp_split_ratio)
        {
            cfg.rules_split_ratio = clamped;
        }
        if let Some(clamped) =
            toml_as_f32(doc.get("groups_split_ratio")).and_then(clamp_split_ratio)
        {
            cfg.groups_split_ratio = clamped;
        }
        if let Some(clamped) =
            toml_as_f32(doc.get("window_w")).and_then(|v| clamp_window_dim(v, 380.0, 7680.0))
        {
            cfg.window_w = clamped;
        }
        if let Some(clamped) =
            toml_as_f32(doc.get("window_h")).and_then(|v| clamp_window_dim(v, 440.0, 4320.0))
        {
            cfg.window_h = clamped;
        }
        if let Some(clamped) = toml_as_f32(doc.get("window_x")).and_then(clamp_window_pos) {
            cfg.window_x = Some(clamped);
        }
        if let Some(clamped) = toml_as_f32(doc.get("window_y")).and_then(clamp_window_pos) {
            cfg.window_y = Some(clamped);
        }
        if let Some(custom_tbl) = doc.get("custom_theme").and_then(toml::Value::as_table) {
            cfg.custom_theme = CustomTheme::from_toml_table_lenient(custom_tbl);
        }
        cfg
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut table = toml::map::Map::new();
        table.insert(
            "theme".to_string(),
            toml::Value::String(self.theme_id.as_str().to_string()),
        );
        table.insert(
            "true_black".to_string(),
            toml::Value::Boolean(self.true_black),
        );
        table.insert(
            "language".to_string(),
            toml::Value::String(self.language.code().to_string()),
        );
        table.insert(
            "auto_check_updates".to_string(),
            toml::Value::Boolean(self.auto_check_updates),
        );
        let round1 = |v: f32| -> f64 { (f64::from(v) * 10.0).round() / 10.0 };
        let round4 = |v: f32| -> f64 { (f64::from(v) * 10_000.0).round() / 10_000.0 };
        table.insert(
            "rules_split_ratio".to_string(),
            toml::Value::Float(round4(self.rules_split_ratio.clamp(0.18, 0.75))),
        );
        table.insert(
            "groups_split_ratio".to_string(),
            toml::Value::Float(round4(self.groups_split_ratio.clamp(0.18, 0.75))),
        );
        table.insert(
            "window_w".to_string(),
            toml::Value::Float(round1(self.window_w.clamp(380.0, 7680.0))),
        );
        table.insert(
            "window_h".to_string(),
            toml::Value::Float(round1(self.window_h.clamp(440.0, 4320.0))),
        );
        if let (Some(x), Some(y)) = (
            self.window_x.and_then(clamp_window_pos),
            self.window_y.and_then(clamp_window_pos),
        ) {
            table.insert("window_x".to_string(), toml::Value::Float(round1(x)));
            table.insert("window_y".to_string(), toml::Value::Float(round1(y)));
        }
        table.insert(
            "custom_theme".to_string(),
            toml::Value::Table(self.custom_theme.to_toml_table()),
        );
        let body = toml::to_string_pretty(&toml::Value::Table(table)).map_err(|e| e.to_string())?;
        fs::write(path, body).map_err(|e| e.to_string())
    }
}

/// Ombra ravvicinata e uniforme su tutti i lati per modali, toast, select e menu contestuali.
///
/// Su temi scuri in modalità True Black (`#000000`) utilizza un leggero alone chiaro al posto del nero,
/// così da separare visivamente menu a tendina e popup dallo sfondo puro.
pub fn uniform_floating_shadow(true_black_dark: bool) -> egui::epaint::Shadow {
    if true_black_dark {
        egui::epaint::Shadow {
            offset: [0, 0],
            blur: 18,
            spread: 3,
            color: egui::Color32::from_rgba_unmultiplied(180, 185, 200, 46),
        }
    } else {
        egui::epaint::Shadow {
            offset: [0, 0],
            blur: 14,
            spread: 2,
            color: egui::Color32::from_black_alpha(110),
        }
    }
}

/// Applica il tema selezionato (e l'eventuale modalità True Black per temi scuri o custom) allo stile egui.
pub fn apply_theme_to_style(style: &mut egui::Style, config: &ThemeConfig, card_radius: u8) {
    let custom = config.custom_theme;
    let radii = config.active_radii(card_radius);
    let (
        is_dark,
        use_true_black,
        raw_bg_panel,
        raw_bg_card,
        raw_bg_input,
        raw_bg_widget,
        accent,
        _secondary_accent,
        text,
        muted_text,
        raw_border,
    ) = match config.theme_id {
        ThemeId::Custom => (
            custom.is_dark,
            custom.is_dark && custom.true_black,
            custom.bg_panel,
            custom.bg_card,
            custom.bg_input,
            custom.bg_widget,
            custom.accent,
            custom.secondary_accent,
            custom.text,
            custom.muted_text,
            custom.border,
        ),
        other => {
            let p = other.preset();
            (
                p.is_dark,
                p.is_dark && config.true_black,
                p.bg_panel,
                p.bg_card,
                p.bg_input,
                p.bg_widget,
                p.accent,
                p.secondary_accent,
                p.text,
                p.muted_text,
                p.border,
            )
        }
    };

    let mut visuals = if is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    let (bg_panel, bg_card, bg_input, bg_widget, border) = if use_true_black {
        (
            egui::Color32::BLACK,
            egui::Color32::from_rgb(10, 10, 12),
            egui::Color32::from_rgb(4, 4, 6),
            egui::Color32::from_rgb(20, 20, 24),
            egui::Color32::from_rgb(38, 38, 44),
        )
    } else {
        (
            raw_bg_panel,
            raw_bg_card,
            raw_bg_input,
            raw_bg_widget,
            raw_border,
        )
    };

    visuals.panel_fill = bg_panel;
    visuals.window_fill = bg_card;
    visuals.extreme_bg_color = bg_input;
    visuals.faint_bg_color = bg_card;
    visuals.window_stroke = egui::Stroke::new(1.0, border);
    visuals.window_corner_radius = egui::CornerRadius::same(radii.modals);
    visuals.menu_corner_radius = egui::CornerRadius::same(radii.cards_menus);
    let floating_shadow = uniform_floating_shadow(use_true_black);
    visuals.window_shadow = floating_shadow;
    visuals.popup_shadow = floating_shadow;
    visuals.override_text_color = Some(text);
    visuals.weak_text_color = Some(muted_text);

    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.hyperlink_color = accent;
    visuals.text_cursor.stroke.color = accent;

    visuals.widgets.noninteractive.bg_fill = bg_card;
    visuals.widgets.noninteractive.weak_bg_fill = bg_card;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, border);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(radii.inner_boxes);

    visuals.widgets.inactive.bg_fill = bg_widget;
    visuals.widgets.inactive.weak_bg_fill = bg_widget;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(radii.controls);

    let hover_bg = if is_dark {
        bg_widget.gamma_multiply(1.22)
    } else {
        bg_widget.gamma_multiply(0.94)
    };
    visuals.widgets.hovered.bg_fill = hover_bg;
    visuals.widgets.hovered.weak_bg_fill = hover_bg;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, text);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(radii.controls);

    let active_bg = if is_dark {
        bg_widget.gamma_multiply(1.35)
    } else {
        bg_widget.gamma_multiply(0.86)
    };
    visuals.widgets.active.bg_fill = active_bg;
    visuals.widgets.active.weak_bg_fill = active_bg;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent);
    // `strong_text_color()` legge `widgets.active.fg_stroke`: deve usare `text`, mai bianco fisso.
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, text);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(radii.controls);

    visuals.widgets.open.bg_fill = bg_card;
    visuals.widgets.open.weak_bg_fill = bg_widget;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, accent);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, text);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(radii.controls);

    style.visuals = visuals;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ten_themes_split_five_dark_five_light_and_gta_vi_present() {
        assert_eq!(ALL_THEMES.len(), 10);
        assert_eq!(ALL_THEMES.iter().filter(|t| t.is_dark).count(), 5);
        assert_eq!(ALL_THEMES.iter().filter(|t| !t.is_dark).count(), 5);
        assert!(
            ALL_THEMES
                .iter()
                .any(|t| t.id == ThemeId::GtaViViceSunset && t.is_dark)
        );
    }

    #[test]
    fn test_theme_application_true_black_and_persistence_roundtrip() {
        let mut style = egui::Style::default();
        let classic = ThemeConfig {
            theme_id: ThemeId::RustBindClassic,
            true_black: false,
            language: Language::English,
            ..ThemeConfig::default()
        };
        apply_theme_to_style(&mut style, &classic, 10);
        assert_eq!(
            style.visuals.selection.bg_fill,
            egui::Color32::from_rgb(183, 65, 14)
        );
        assert_eq!(
            style.visuals.menu_corner_radius,
            egui::CornerRadius::same(10)
        );
        assert_eq!(style.visuals.popup_shadow.color.r(), 0);

        let true_black_gta = ThemeConfig {
            theme_id: ThemeId::GtaViViceSunset,
            true_black: true,
            custom_theme: CustomTheme::default(),
            language: Language::Italian,
            auto_check_updates: false,
            rules_split_ratio: 0.42,
            groups_split_ratio: 0.28,
            window_w: 1024.0,
            window_h: 700.0,
            window_x: Some(120.0),
            window_y: Some(80.0),
        };
        apply_theme_to_style(&mut style, &true_black_gta, 10);
        assert_eq!(style.visuals.panel_fill, egui::Color32::BLACK);
        assert!(style.visuals.popup_shadow.color.r() > 0);

        // True Black non deve scurire un tema chiaro e strong_text_color non deve mai restare bianco
        for preset in ALL_THEMES.iter().filter(|p| !p.is_dark) {
            let light_with_flag = ThemeConfig {
                theme_id: preset.id,
                true_black: true,
                language: Language::French,
                ..ThemeConfig::default()
            };
            apply_theme_to_style(&mut style, &light_with_flag, 10);
            assert!(!style.visuals.dark_mode);
            assert_ne!(style.visuals.panel_fill, egui::Color32::BLACK);
            assert_ne!(style.visuals.strong_text_color(), egui::Color32::WHITE);
            assert_ne!(style.visuals.text_color(), egui::Color32::WHITE);
        }

        let dir = std::env::temp_dir().join(format!("rustbind_theme_test_{}", std::process::id()));
        let path = dir.join(UI_PREFERENCES_FILENAME);
        true_black_gta.save_to_file(&path).expect("Salva tema");
        let loaded = ThemeConfig::load_from_file(&path);
        assert_eq!(loaded, true_black_gta);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_custom_theme_apply_true_black_radii_and_toml_roundtrip() {
        let mut style = egui::Style::default();
        let custom = CustomTheme {
            is_dark: true,
            true_black: true,
            bg_panel: egui::Color32::from_rgb(0x10, 0x20, 0x30),
            bg_card: egui::Color32::from_rgb(0x20, 0x30, 0x40),
            bg_input: egui::Color32::from_rgb(0x05, 0x06, 0x07),
            bg_widget: egui::Color32::from_rgb(0x30, 0x40, 0x50),
            accent: egui::Color32::from_rgb(0xAA, 0x55, 0x22),
            secondary_accent: egui::Color32::from_rgb(0x22, 0xBB, 0xCC),
            text: egui::Color32::from_rgb(0xF1, 0xF2, 0xF3),
            muted_text: egui::Color32::from_rgb(0x90, 0x95, 0xA0),
            border: egui::Color32::from_rgb(0x44, 0x45, 0x46),
            radii: ThemeBorderRadii {
                controls: 4,
                inner_boxes: 7,
                cards_menus: 12,
                modals: 18,
            },
        };
        let cfg = ThemeConfig {
            theme_id: ThemeId::Custom,
            true_black: false,
            custom_theme: custom,
            ..ThemeConfig::default()
        };
        apply_theme_to_style(&mut style, &cfg, 10);
        assert!(style.visuals.dark_mode);
        assert_eq!(style.visuals.panel_fill, egui::Color32::BLACK);
        assert_eq!(style.visuals.selection.bg_fill, custom.accent);
        assert_eq!(style.visuals.override_text_color, Some(custom.text));
        assert_eq!(
            style.visuals.widgets.inactive.corner_radius,
            egui::CornerRadius::same(4)
        );
        assert_eq!(
            style.visuals.widgets.noninteractive.corner_radius,
            egui::CornerRadius::same(7)
        );
        assert_eq!(
            style.visuals.menu_corner_radius,
            egui::CornerRadius::same(12)
        );
        assert_eq!(
            style.visuals.window_corner_radius,
            egui::CornerRadius::same(18)
        );
        assert_eq!(cfg.active_radii(10).modals, 18);

        let light_custom_cfg = ThemeConfig {
            theme_id: ThemeId::Custom,
            custom_theme: CustomTheme {
                is_dark: false,
                true_black: true,
                bg_panel: egui::Color32::from_rgb(0xFA, 0xFA, 0xFC),
                text: egui::Color32::from_rgb(0x11, 0x12, 0x13),
                ..custom
            },
            ..ThemeConfig::default()
        };
        apply_theme_to_style(&mut style, &light_custom_cfg, 10);
        assert!(!style.visuals.dark_mode);
        assert_eq!(
            style.visuals.panel_fill,
            egui::Color32::from_rgb(0xFA, 0xFA, 0xFC)
        );

        let toml_text = custom.to_toml_string().expect("serialize custom theme");
        let parsed = CustomTheme::from_toml_str(&toml_text).expect("parse custom theme");
        assert_eq!(parsed, custom);
        assert!(CustomTheme::from_toml_str("not = valid_toml [").is_err());
        assert!(CustomTheme::from_toml_str("schema_version = 1\nis_dark = true").is_err());
    }
}
