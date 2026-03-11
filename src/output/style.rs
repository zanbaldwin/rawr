use console::Style;

pub(crate) struct Palette {
    pub heading: Style,
    pub success: Style,
    pub warning: Style,
    pub danger: Style,
    pub muted: Style,
    pub highlight: Style,
    pub added: Style,
    pub removed: Style,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            heading: Style::new().green().bold(),
            success: Style::new().green(),
            warning: Style::new().yellow(),
            danger: Style::new().red(),
            muted: Style::new().dim(),
            highlight: Style::new().white().bold(),
            added: Style::new().green(),
            removed: Style::new().red(),
        }
    }
}
