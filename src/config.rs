use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Custom,
}

impl Anchor {
    fn as_str(&self) -> &'static str {
        match self {
            Anchor::TopLeft => "top-left",
            Anchor::TopCenter => "top-center",
            Anchor::TopRight => "top-right",
            Anchor::BottomLeft => "bottom-left",
            Anchor::BottomCenter => "bottom-center",
            Anchor::BottomRight => "bottom-right",
            Anchor::Custom => "custom",
        }
    }
    fn parse(s: &str) -> Anchor {
        match s {
            "top-left" => Anchor::TopLeft,
            "top-center" => Anchor::TopCenter,
            "top-right" => Anchor::TopRight,
            "bottom-left" => Anchor::BottomLeft,
            "bottom-center" => Anchor::BottomCenter,
            "bottom-right" => Anchor::BottomRight,
            "custom" => Anchor::Custom,
            _ => Anchor::TopLeft,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub anchor: Anchor,
    pub margin_x: i32,
    pub margin_y: i32,
    pub custom_x: i32,
    pub custom_y: i32,
    pub hold_ms: u32,
    pub compact: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            anchor: Anchor::TopLeft,
            margin_x: 20,
            margin_y: 20,
            custom_x: 100,
            custom_y: 100,
            hold_ms: 3000,
            compact: false,
        }
    }
}

static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();

fn slot() -> &'static Mutex<Config> {
    CONFIG.get_or_init(|| Mutex::new(load_or_default()))
}

pub fn get() -> Config {
    *slot().lock().unwrap()
}

pub fn set(new_cfg: Config) {
    if let Ok(mut guard) = slot().lock() {
        *guard = new_cfg;
    }
    let _ = save(&new_cfg);
}

fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let mut p = PathBuf::from(appdata);
    p.push("FlyoutLite");
    let _ = fs::create_dir_all(&p);
    p.push("config.ini");
    Some(p)
}

fn load_or_default() -> Config {
    let mut cfg = Config::default();
    let Some(path) = config_path() else { return cfg };
    let Ok(text) = fs::read_to_string(&path) else { return cfg };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim();
        let value = value.trim();
        match key {
            "anchor" => cfg.anchor = Anchor::parse(value),
            "margin_x" => cfg.margin_x = value.parse().unwrap_or(cfg.margin_x),
            "margin_y" => cfg.margin_y = value.parse().unwrap_or(cfg.margin_y),
            "custom_x" => cfg.custom_x = value.parse().unwrap_or(cfg.custom_x),
            "custom_y" => cfg.custom_y = value.parse().unwrap_or(cfg.custom_y),
            "hold_ms" => cfg.hold_ms = value.parse().unwrap_or(cfg.hold_ms),
            "compact" => cfg.compact = value == "true" || value == "1",
            _ => {}
        }
    }
    cfg
}

fn save(cfg: &Config) -> std::io::Result<()> {
    let Some(path) = config_path() else {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "no APPDATA"));
    };
    let text = format!(
        "anchor={}\nmargin_x={}\nmargin_y={}\ncustom_x={}\ncustom_y={}\nhold_ms={}\ncompact={}\n",
        cfg.anchor.as_str(),
        cfg.margin_x,
        cfg.margin_y,
        cfg.custom_x,
        cfg.custom_y,
        cfg.hold_ms,
        cfg.compact,
    );
    fs::write(path, text)
}
