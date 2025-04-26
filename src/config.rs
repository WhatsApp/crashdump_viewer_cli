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

use serde::Deserialize;
use ratatui::style::Color;
use std::fs;

// default color TOML
/*
[general]
background = "Blue"
highlight = "Cyan"
header = "Red"

[process]
background = "Green"
highlight = "Yellow"
header = "Red"

[process_group]
background = "Purple"
highlight = "Magenta"
header = "Red"

[inspect]
background = "Orange"
highlight = "Brown"
header = "Red"

[common]
default_text = "White"
highlight_text = "Blue"
header_text = "Red"
header_background = "DarkGray"
highlight_background = "LightGray"
error_text = "Red"
warning_text = "Yellow"
info_text = "Cyan"
*/

#[derive(Deserialize, Clone)]
struct ColorConfig {
    general: ColorScheme,
    process: ColorScheme,
    process_group: ColorScheme,
    inspect: ColorScheme,
    common: CommonColors,
}

#[derive(Deserialize, Clone)]
struct ColorScheme {
    text: Color,
    background: Color,
    highlight: Color,
    header: Color,
}

#[derive(Deserialize, Clone)]
struct CommonColors {
    default_text: Color,
    highlight_text: Color,
    header_text: Color,
    header_background: Color,
    highlight_background: Color,
    error_text: Color,
    warning_text: Color,
    info_text: Color,
}


impl ColorConfig {
    fn load_from_file(file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config_data = fs::read_to_string(file_path)?;
        let config: ColorConfig = toml::from_str(&config_data)?;
        Ok(config)
    }
}
