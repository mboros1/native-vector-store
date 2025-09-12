#[derive(Clone, Copy)]
pub struct TextState {
    pub font_size: f64,
    pub char_spacing: f64,
    pub word_spacing: f64,
    pub h_scale: f64,
    pub leading: f64,
    pub tm_e: f64,
    pub tm_f: f64,
    pub prev_y: Option<f64>,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            font_size: 12.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            h_scale: 1.0,
            leading: 0.0,
            tm_e: 0.0,
            tm_f: 0.0,
            prev_y: None,
        }
    }
}

