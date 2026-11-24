//! Instants and spans in the recording, in seconds.

use serde::{Deserialize, Serialize};

/// A half-open range of instants, `start <= t < end`, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: f64,
    pub end: f64,
}

impl Span {
    pub fn new(start: f64, end: f64) -> Self {
        Span { start, end }
    }

    pub fn contains(&self, t: f64) -> bool {
        t >= self.start && t < self.end
    }

    pub fn duration(&self) -> f64 {
        self.end - self.start
    }
}

/// `m:ss` or `h:mm:ss`, for headings.
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}
