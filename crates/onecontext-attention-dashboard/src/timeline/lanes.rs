use eframe::egui::Color32;

use crate::schema::TimelineLaneConfig;

#[derive(Debug, Clone)]
pub struct TimelineLane {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub visible: bool,
    pub color: Color32,
    pub source_ref: Option<String>,
}

impl TimelineLane {
    pub fn from_config(config: &TimelineLaneConfig) -> Self {
        Self {
            id: config.id.clone(),
            title: config.title.clone(),
            kind: config.kind.clone(),
            visible: config.visible,
            color: parse_color(&config.color).unwrap_or(Color32::LIGHT_BLUE),
            source_ref: config.source_ref.clone(),
        }
    }

    pub fn event_color(&self, selected: bool) -> Color32 {
        if selected {
            Color32::WHITE
        } else {
            self.color
        }
    }

    pub fn soft_color(&self) -> Color32 {
        Color32::from_rgba_premultiplied(self.color.r(), self.color.g(), self.color.b(), 36)
    }
}

fn parse_color(value: &str) -> Option<Color32> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}
