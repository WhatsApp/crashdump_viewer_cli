// Copyright (c) Meta Platforms, Inc. and affiliates.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at

//     http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::io;

use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    app::{App, AppResult, AppState},
    event::{Event, EventHandler},
    handler::handle_key_events,
    tui::Tui,
};

pub mod app;
pub mod event;
pub mod handler;
mod parser;
pub mod tui;
pub mod ui;

use clap::Parser;
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Action to perform. Should be one of "tui", "json"
    /// Default is "tui", which launches the TUI
    /// "json" will print a JSON representation
    #[arg(short, long, default_value_t = String::from("tui"))]
    action: String,

    /// Path to the crash dump
    #[arg(required = true)]
    filepath: String,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let args = Args::parse();

    if args.action == "tui" {
        // Create an application.
        let mut app = App::new(args.filepath);

        // Initialize the terminal user interface.
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        let events = EventHandler::new(250);
        let mut tui = Tui::new(terminal, events);
        tui.init()?;

        // Start the main loop.
        while app.state == AppState::Running {
            // Render the user interface.
            tui.draw(&mut app)?;
            // Handle events.
            match tui.events.next().await? {
                Event::Tick => app.tick(),
                Event::Key(key_event) => handle_key_events(key_event, &mut app)?,
                Event::Mouse(_) => {}
                Event::Resize(_, _) => {}
            }
        }

        // Exit the user interface.
        tui.exit()?;
    } else if args.action == "json" {
        todo!()
    } else {
        println!("Invalid action: {}", args.action);
    }

    Ok(())
}
