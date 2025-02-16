use std::error;
use std::collections::HashMap;
use color_eyre::Result;
use ratatui::{
    buffer::Buffer,
    style::{palette::tailwind, Color, Stylize},
    widgets::{Block, Padding, Paragraph, Tabs, Widget, List, ListDirection},
    text::Line,
    layout::{Constraint, Layout, Rect},
    symbols,
};
use strum::{IntoEnumIterator};
use strum_macros::{Display, EnumIter, FromRepr};

/// Application result type.
pub type AppResult<T> = std::result::Result<T, Box<dyn error::Error>>;

/// Application.
pub struct App {
    /// header
    pub header: String,
    pub state: AppState,
    pub selected_tab: SelectedTab,

    pub index: Vec<String>,
    pub general_info: HashMap<String, String>,
    pub process_info: HashMap<String, String>,
    pub binary_info: HashMap<String, String>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    #[default]
    Running,
    Quitting
}

#[derive(Default, Clone, Copy, Display, FromRepr, EnumIter)]
enum SelectedTab {
    #[default]
    #[strum(to_string = "General Information")]
    General,
    #[strum(to_string = "Index")]
    Index,
    #[strum(to_string = "Process Information")]
    Process,
    #[strum(to_string = "Binary Information")]
    Binary
}

impl Default for App {
    fn default() -> Self {
        Self {
            state: AppState::Running,
            selected_tab: SelectedTab::General,
            index: vec![],
            general_info: HashMap::new(),
            process_info: HashMap::new(),
            binary_info: HashMap::new(),   
            header: "ERL CRASH DUMP VIEWER".to_string(),
        }
    }
}

impl App {
    /// Constructs a new instance of [`App`].
    pub fn new(idx: Vec<String>) -> Self {
        let mut ret = Self::default();
        ret.index = idx;
        ret
    }

    /// Handles the tick event of the terminal.
    pub fn tick(&self) {}

    /// Set running to false to quit the application.
    pub fn quit(&mut self) {
        self.state = AppState::Quitting;
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = self.selected_tab.next()
    }

    pub fn prev_tab(&mut self) {
        self.selected_tab = self.selected_tab.previous()
    }

}

// Separated because this is the UI code. We need this here in order to render stuff *within* App state
impl App {
    pub fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        let titles = SelectedTab::iter().map(SelectedTab::title);
        let highlight_style = (Color::default(), self.selected_tab.palette().c700);
        let selected_tab_index = self.selected_tab as usize;
        Tabs::new(titles)
            .highlight_style(highlight_style)
            .select(selected_tab_index)
            .padding("", "")
            .divider(" ")
            .render(area, buf);
    }

}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use Constraint::{Length, Min};
        let vertical = Layout::vertical([Length(1), Min(0), Length(1)]);
        let [header_area, inner_area, footer_area] = vertical.areas(area);

        let horizontal = Layout::horizontal([Min(0), Length(20)]);
        let [tabs_area, title_area] = horizontal.areas(header_area);

        render_title(title_area, buf);
        self.render_tabs(tabs_area, buf);
        match self.selected_tab {
            SelectedTab::General => self.selected_tab.render_general(inner_area, buf, self),
            SelectedTab::Index => self.selected_tab.render_index(inner_area, buf, self),
            SelectedTab::Process => self.selected_tab.render_process(inner_area, buf, self),
            SelectedTab::Binary => self.selected_tab.render_binary(inner_area, buf, self),
        }
        render_footer(footer_area, buf);
    }
}


impl SelectedTab {
    /// Get the previous tab, if there is no previous tab return the current tab.
    fn previous(self) -> Self {
        let current_index: usize = self as usize;
        let previous_index = current_index.saturating_sub(1);
        Self::from_repr(previous_index).unwrap_or(self)
    }

    /// Get the next tab, if there is no next tab return the current tab.
    fn next(self) -> Self {
        let current_index = self as usize;
        let next_index = current_index.saturating_add(1);
        Self::from_repr(next_index).unwrap_or(self)
    }
}

// impl Widget for SelectedTab {
//     fn render(self, area: Rect, buf: &mut Buffer) {
//         // in a real app these might be separate widgets
//         match self {
//             Self::General => self.render_general(area, buf),
//             Self::Index => self.render_index(area, buf),
//             Self::Process => self.render_process(area, buf),
//             Self::Binary => self.render_binary(area, buf),
//         }
//     }
// }

impl SelectedTab {
    /// Return tab's name as a styled `Line`
    fn title(self) -> Line<'static> {
        format!("  {self}  ")
            .fg(tailwind::SLATE.c200)
            .bg(self.palette().c900)
            .into()
    }

    fn render_general(self, area: Rect, buf: &mut Buffer, app: &App) {
        Paragraph::new("Hello, World!")
            .block(self.block())
            .render(area, buf);
    }

    fn render_index(self, area: Rect, buf: &mut Buffer, app: &App) {
        List::new(app.index.clone())
            .block(Block::bordered().title("List"))
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true)
            .direction(ListDirection::BottomToTop)
            .render(area, buf);
    }

    fn render_process(self, area: Rect, buf: &mut Buffer, app: &App) {
        Paragraph::new("Welcome to the Ratatui tabs example!")
            .block(self.block())
            .render(area, buf);
    }

    fn render_binary(self, area: Rect, buf: &mut Buffer, app: &App) {
        Paragraph::new("This is the third tab!")
            .block(self.block())
            .render(area, buf);
    }

    /// A block surrounding the tab's content
    fn block(self) -> Block<'static> {
        Block::bordered()
            .border_set(symbols::border::PROPORTIONAL_TALL)
            .padding(Padding::horizontal(1))
            .border_style(self.palette().c700)
    }

    const fn palette(self) -> tailwind::Palette {
        match self {
            Self::General => tailwind::BLUE,
            Self::Index => tailwind::TEAL,
            Self::Process => tailwind::EMERALD,
            Self::Binary => tailwind::INDIGO,
        }
    }
}

fn render_title(area: Rect, buf: &mut Buffer) {
    "ERL Crash Dump Viewer".render(area, buf);
}

fn render_footer(area: Rect, buf: &mut Buffer) {
    Line::raw("< > to change tabs | Press q to quit")
        .centered()
        .render(area, buf);
}
