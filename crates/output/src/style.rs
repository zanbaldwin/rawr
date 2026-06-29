use console::Style;
use std::sync::LazyLock;

pub static PALETTE: LazyLock<Palette> = LazyLock::new(Palette::default);

pub struct Palette {
    pub heading: Style,
    pub success: Style,
    pub warning: Style,
    pub danger: Style,
    pub muted: Style,
    pub highlight: Style,
    pub label: Style,
    pub accent: Style,
    pub added: Style,
    pub removed: Style,
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            heading: Style::new().green().bold().underlined(),
            success: Style::new().green(),
            warning: Style::new().yellow(),
            danger: Style::new().red(),
            muted: Style::new().dim(),
            highlight: Style::new().bright().bold(),
            label: Style::new().cyan(),
            accent: Style::new().magenta(),
            added: Style::new().green(),
            removed: Style::new().red(),
        }
    }
}
