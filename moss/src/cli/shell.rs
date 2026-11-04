// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

//! Renders [`moss::Event`]s for the shell

use std::sync::Arc;

use moss::event;

pub use self::human::Human;
pub use self::json::Json;

mod human;
mod json;

/// How events are written to stdout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Format {
    /// Progress bars and messages
    #[default]
    Human,
    /// One json object per line
    Json,
}

impl Format {
    /// The observer that writes events in this format
    pub fn observer(self) -> Arc<dyn event::Observer> {
        match self {
            Format::Human => Arc::new(Human::default()),
            Format::Json => Arc::new(Json),
        }
    }
}
