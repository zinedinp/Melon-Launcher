//! The client jar assets are extracted from, and whether it is the official one.

use super::kit::theme::{self, Color};
use super::kit::{Cx, Rect};
use melon_base::prepare::hashes::Verdict;
use melon_base::prepare::{self, Found};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};

/// What checking the jar against the official hash found.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HashStatus {
    Idle,
    Checking,
    Match,
    Mismatch,
    Unknown,
}

pub struct JarSource {
    pub found: Option<Found>,
    pub hash: HashStatus,
    hash_rx: Option<Receiver<Verdict>>,
}

impl JarSource {
    /// Looks for a client jar on this machine.
    pub fn scan(home: &Path) -> Self {
        let mut jar = Self {
            found: None,
            hash: HashStatus::Idle,
            hash_rx: None,
        };
        jar.set(prepare::scan(home));
        jar
    }

    /// The jar changed: checks it against the official hash.
    pub fn set(&mut self, found: Option<Found>) {
        let sha1 = found.as_ref().and_then(|f| prepare::sha1_file(&f.jar));
        self.found = found;
        self.hash_rx = None;
        self.hash = HashStatus::Idle;
        if let Some(sha1) = sha1 {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(prepare::hashes::verify(melon_base::home::VERSION, &sha1));
            });
            self.hash_rx = Some(rx);
            self.hash = HashStatus::Checking;
        }
    }

    /// Takes in the hash check's answer.
    pub fn poll(&mut self) {
        if let Some(verdict) = self.hash_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.hash_rx = None;
            self.hash = match verdict {
                Verdict::Match => HashStatus::Match,
                Verdict::Mismatch { .. } => HashStatus::Mismatch,
                Verdict::Unknown | Verdict::Offline => HashStatus::Unknown,
            };
        }
    }

    /// Not a jar Mojang shipped: extracting from it is blocked.
    pub fn rejected(&self) -> bool {
        self.hash == HashStatus::Mismatch
    }

    fn hash_line(&self) -> Option<(&'static str, Color)> {
        match self.hash {
            HashStatus::Idle => None,
            HashStatus::Checking => Some(("assets.hash_checking", theme::DIM)),
            HashStatus::Match => Some(("assets.hash_ok", theme::ACCENT)),
            HashStatus::Mismatch => Some(("assets.hash_bad", theme::DANGER)),
            HashStatus::Unknown => Some(("assets.hash_unknown", theme::DIM)),
        }
    }

    /// Where the jar is and what its hash says, or `missing` when there is none. Returns the y
    /// below it.
    pub fn draw(&self, cx: &mut Cx, x: f32, y: f32, w: f32, missing: &str) -> f32 {
        let Some(found) = &self.found else {
            return y + cx.paragraph(missing, x, y, w, theme::WARN) + 14.0;
        };
        cx.label(&found.label, x, y, w, theme::TEXT);
        let path = found.jar.display().to_string();
        cx.small_label(&path, x, y + 11.0, w, theme::DIM);
        cx.tooltip(Rect::new(x, y + 10.0, w, 9.0), &path);
        let mut y = y + 23.0;
        if let Some((key, color)) = self.hash_line() {
            cx.small_label(cx.t(key), x, y, w, color);
        }
        y += 12.0;
        y
    }
}
