use ratatui::{
    style::{Color, Style},
    widgets::{Block, BorderType, Paragraph, Widget},
    Frame,
    text::Line,
    layout::{Constraint, Layout, Rect, Alignment},
    buffer::Buffer
};

use crate::app::App;

/// Renders the user interface widgets.
pub fn render(app: &mut App, frame: &mut Frame) {
    // This is where you add new widgets.
    // See the following resources:
    // - https://docs.rs/ratatui/latest/ratatui/widgets/index.html
    // - https://github.com/ratatui/ratatui/tree/master/examples
    frame.render_widget(
        app,
        frame.area(),
    )
}
