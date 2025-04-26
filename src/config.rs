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

use ratatui::style::Color;

// Kept as CommonColors to match app.rs for now.
#[derive(Clone, Debug)] // Added Debug
pub struct CommonColors {
    pub default_text: Color,
    pub highlight_text: Color, // e.g., for selected items text
    pub header_text: Color,
    pub header_background: Color,
    pub highlight_background: Color, // e.g., for selected items background
    pub error_text: Color,
    pub warning_text: Color,
    pub info_text: Color,
    // Add other commonly used colors if needed
    pub border_color: Color,
    pub title_color: Color,
}

impl Default for CommonColors {
    fn default() -> Self {
        Self {
            // Using standard Ratatui colors for defaults
            default_text: Color::White,
            highlight_text: Color::Yellow,
            header_text: Color::White,
            header_background: Color::Blue,
            highlight_background: Color::DarkGray,
            error_text: Color::Red,
            warning_text: Color::Yellow,
            info_text: Color::Cyan,
            border_color: Color::Gray,
            title_color: Color::White,
        }
    }
}
