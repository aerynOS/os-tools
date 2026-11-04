// SPDX-FileCopyrightText: 2026 aerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::io::{self, Write};

use moss::{Event, event};

/// Writes each event to stdout as a line of json
pub struct Json;

impl event::Observer for Json {
    fn event(&self, event: &Event) {
        let mut stdout = io::stdout().lock();
        let _ = serde_json::to_writer(&mut stdout, event);
        let _ = writeln!(stdout);
    }
}
