//! Small state machines behind status text and image retries. These hold the fixes for old-app bugs
//! that were about state, not drawing, so they can be tested without a shell.

use crate::commands::strings;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusMessage {
    pub text: String,
    /// `None` stays until replaced.
    pub timeout_ms: Option<u32>,
}

impl StatusMessage {
    fn sticky(text: impl Into<String>) -> Self {
        StatusMessage {
            text: text.into(),
            timeout_ms: None,
        }
    }
    fn transient(text: impl Into<String>) -> Self {
        StatusMessage {
            text: text.into(),
            timeout_ms: Some(strings::TRANSIENT_MS),
        }
    }
}

/// How a reload attempt ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadOutcome {
    Ok {
        warnings: Vec<String>,
    },
    Failed(String),
    /// The file is gone. `had_content` says whether earlier content is still on screen.
    Missing {
        had_content: bool,
    },
}

/// Tracks reloads of one tab. Every `begin` supersedes the previous one, and each generation reports
/// at most one final message, so a late or stale finish cannot overwrite what the user was told.
#[derive(Debug, Default)]
pub struct ReloadTracker {
    generation: u64,
    running: Option<u64>,
    /// A first load has nothing to be refreshed from, so it does not announce itself.
    loaded: bool,
    /// The last message this tracker gave stays until replaced.
    sticky_showing: bool,
}

impl ReloadTracker {
    /// Returns the generation and the message to show now. Only a manual refresh announces itself:
    /// a text change on every automatic reload would shift the layout for nothing.
    pub fn begin(&mut self, manual: bool) -> (u64, Option<StatusMessage>) {
        self.generation += 1;
        self.running = Some(self.generation);
        self.sticky_showing |= manual;
        (
            self.generation,
            manual.then(|| StatusMessage::sticky(strings::REFRESHING)),
        )
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// The deadline passed. Later `finished` calls for this generation change nothing.
    pub fn timed_out(&mut self, generation: u64) -> Option<StatusMessage> {
        self.take_current(generation)?;
        self.sticky_showing = true;
        Some(StatusMessage::sticky(strings::RENDER_TIMEOUT))
    }

    /// Always replaces "Refreshing document…" for the current generation, whatever the outcome. A
    /// tab's first successful load says nothing: `None`, or empty text if an earlier message has to go.
    pub fn finished(&mut self, generation: u64, outcome: ReloadOutcome) -> Option<StatusMessage> {
        self.take_current(generation)?;
        let first_load = !self.loaded && matches!(outcome, ReloadOutcome::Ok { .. });
        self.loaded |= first_load;
        let message = match outcome {
            ReloadOutcome::Ok { warnings } if !warnings.is_empty() => {
                StatusMessage::sticky(warnings.join("\n"))
            }
            ReloadOutcome::Ok { .. } if first_load => {
                if !self.sticky_showing {
                    return None;
                }
                StatusMessage::sticky("")
            }
            ReloadOutcome::Ok { .. } => StatusMessage::transient(strings::REFRESHED),
            ReloadOutcome::Failed(e) => StatusMessage::sticky(e),
            ReloadOutcome::Missing { had_content: true } => {
                StatusMessage::sticky(strings::FILE_DELETED)
            }
            ReloadOutcome::Missing { had_content: false } => {
                StatusMessage::sticky(strings::FILE_NOT_FOUND)
            }
        };
        self.sticky_showing = message.timeout_ms.is_none() && !message.text.is_empty();
        Some(message)
    }

    fn take_current(&mut self, generation: u64) -> Option<u64> {
        self.running.take_if(|r| *r == generation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FetchState {
    Pending,
    FailedAt(Instant),
}

/// Remote image requests. A failed URL may be retried after a delay or as soon as the document
/// reloads; the old app's `pending` set never forgot one, so a failure lasted for the page's life.
#[derive(Debug)]
pub struct ImageRequests {
    states: HashMap<String, FetchState>,
    retry_after: Duration,
}

impl Default for ImageRequests {
    fn default() -> Self {
        ImageRequests {
            states: HashMap::new(),
            retry_after: Duration::from_secs(30),
        }
    }
}

impl ImageRequests {
    pub fn with_retry_after(retry_after: Duration) -> Self {
        ImageRequests {
            states: HashMap::new(),
            retry_after,
        }
    }

    /// True if the caller should start a fetch now. The URL then counts as pending.
    pub fn should_fetch(&mut self, url: &str, now: Instant) -> bool {
        let allowed = match self.states.get(url) {
            None => true,
            Some(FetchState::Pending) => false,
            Some(FetchState::FailedAt(t)) => now.duration_since(*t) >= self.retry_after,
        };
        if allowed {
            self.states.insert(url.to_string(), FetchState::Pending);
        }
        allowed
    }

    /// Success forgets the URL, because the decoded image lives in the image cache from then on.
    pub fn finished(&mut self, url: &str, ok: bool, now: Instant) {
        if ok {
            self.states.remove(url);
        } else {
            self.states
                .insert(url.to_string(), FetchState::FailedAt(now));
        }
    }

    /// A reload is an explicit reason to try every failed image again.
    pub fn on_reload(&mut self) {
        self.states.retain(|_, s| matches!(s, FetchState::Pending));
    }
}
