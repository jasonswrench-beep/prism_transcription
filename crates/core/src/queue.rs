//! The batch queue (T8): which files are waiting to be transcribed, in what order, and how each one ended. It lives
//! in the project file, so it survives a restart or a crash; the transcription job itself keeps its checkpoints, so a
//! file picks up from its last finished chunk. Each file keeps its own settings (its import fields). Running the
//! queue is the app's job; this is the state and its rules.

use crate::project::now;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueState {
    #[default]
    Idle,
    Running,
    Paused,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    #[default]
    Waiting,
    Running,
    /// Kept in the queue but skipped until released (pausing one file).
    Held,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueueItem {
    /// The media file's id.
    pub id: String,
    #[serde(default)]
    pub state: ItemState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub added: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Queue {
    #[serde(default)]
    pub state: QueueState,
    #[serde(default)]
    pub items: Vec<QueueItem>,
    /// Set when the app found the queue running at open: it had been stopped by a crash or by closing the app.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupted: bool,
    /// Raised by every change, so the window can tell a newer state from an older one arriving late.
    #[serde(default)]
    pub rev: u64,
}

impl Queue {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Nothing queued and not running: left out of the project file.
    pub fn is_unused(&self) -> bool {
        // Once used, it stays in the file, so its revision never goes back.
        self.items.is_empty() && self.state == QueueState::Idle && !self.interrupted && self.rev == 0
    }

    /// Add files at the end. A file already waiting, running or held stays where it is; a finished one is queued
    /// again. Returns how many were added.
    pub fn add(&mut self, ids: &[String]) -> usize {
        let mut n = 0;
        for id in ids {
            if self.items.iter().any(|i| &i.id == id && matches!(i.state, ItemState::Waiting | ItemState::Running | ItemState::Held)) {
                continue;
            }
            self.items.retain(|i| &i.id != id);
            self.items.push(QueueItem { id: id.clone(), state: ItemState::Waiting, error: None, added: now(), finished: None });
            n += 1;
        }
        n
    }

    /// The next file to run, marked running; none when the queue is not running or nothing waits.
    pub fn take_next(&mut self) -> Option<String> {
        if self.state != QueueState::Running {
            return None;
        }
        let it = self.items.iter_mut().find(|i| i.state == ItemState::Waiting)?;
        it.state = ItemState::Running;
        it.error = None;
        Some(it.id.clone())
    }

    pub fn running(&self) -> Option<&QueueItem> {
        self.items.iter().find(|i| i.state == ItemState::Running)
    }

    fn item(&mut self, id: &str) -> Option<&mut QueueItem> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    /// How a run ended. A run stopped by Pause goes back to waiting (it resumes from its checkpoint); one stopped
    /// because the user held or removed it was already updated by that command.
    pub fn finish(&mut self, id: &str, result: Result<(), String>, cancelled: bool) {
        let paused = self.state != QueueState::Running;
        let Some(it) = self.item(id) else { return };
        if it.state != ItemState::Running {
            return;
        }
        match (cancelled, result) {
            (true, _) if paused => it.state = ItemState::Waiting,
            (true, _) => it.state = ItemState::Cancelled,
            (false, Ok(())) => {
                it.state = ItemState::Done;
                it.finished = Some(now());
            }
            (false, Err(e)) => {
                it.state = ItemState::Failed;
                it.error = Some(e);
                it.finished = Some(now());
            }
        }
    }

    /// Whether anything is left to run.
    pub fn has_waiting(&self) -> bool {
        self.items.iter().any(|i| i.state == ItemState::Waiting)
    }

    pub fn hold(&mut self, id: &str, hold: bool) -> bool {
        match self.item(id) {
            Some(it) if hold && matches!(it.state, ItemState::Waiting | ItemState::Running) => it.state = ItemState::Held,
            Some(it) if !hold && it.state == ItemState::Held => it.state = ItemState::Waiting,
            _ => return false,
        }
        true
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let n = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() != n
    }

    /// Move a file to position `to` (drag and drop, or the move buttons).
    pub fn move_to(&mut self, id: &str, to: usize) -> bool {
        let Some(from) = self.items.iter().position(|i| i.id == id) else { return false };
        let it = self.items.remove(from);
        self.items.insert(to.min(self.items.len()), it);
        true
    }

    /// Cancel the whole queue: everything not finished leaves it.
    pub fn cancel_all(&mut self) {
        self.state = QueueState::Idle;
        self.interrupted = false;
        self.items.retain(|i| matches!(i.state, ItemState::Done | ItemState::Failed));
    }

    /// Remove the finished, failed and cancelled entries.
    pub fn clear_finished(&mut self) {
        self.items.retain(|i| matches!(i.state, ItemState::Waiting | ItemState::Running | ItemState::Held));
    }

    /// On opening a project: a queue still marked running was stopped by a crash or by closing the app. Its running
    /// file goes back to waiting, and the queue waits for the user to resume it.
    pub fn recover(&mut self) -> bool {
        let mut changed = false;
        for it in &mut self.items {
            if it.state == ItemState::Running {
                it.state = ItemState::Waiting;
                changed = true;
            }
        }
        if self.state == QueueState::Running {
            self.state = QueueState::Paused;
            self.interrupted = true;
            changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(q: &Queue) -> Vec<(String, ItemState)> {
        q.items.iter().map(|i| (i.id.clone(), i.state)).collect()
    }

    #[test]
    fn a_queue_runs_in_order_and_records_how_each_ended() {
        let mut q = Queue::default();
        assert_eq!(q.add(&["F001".into(), "F002".into(), "F003".into()]), 3);
        assert_eq!(q.add(&["F002".into()]), 0, "already waiting");
        assert!(q.take_next().is_none(), "not started");
        q.state = QueueState::Running;
        assert!(q.move_to("F003", 0));
        assert_eq!(q.take_next().as_deref(), Some("F003"));
        q.finish("F003", Ok(()), false);
        assert_eq!(q.take_next().as_deref(), Some("F001"));
        q.finish("F001", Err("could not decode".into()), false);
        assert_eq!(q.take_next().as_deref(), Some("F002"));
        // Pause while F002 runs: it goes back to waiting and starts again from its checkpoint on resume.
        q.state = QueueState::Paused;
        q.finish("F002", Err("cancelled".into()), true);
        assert_eq!(ids(&q), [("F003".into(), ItemState::Done), ("F001".into(), ItemState::Failed), ("F002".into(), ItemState::Waiting)]);
        assert!(q.take_next().is_none(), "paused");
        q.state = QueueState::Running;
        assert_eq!(q.take_next().as_deref(), Some("F002"));
        q.finish("F002", Ok(()), false);
        assert!(!q.has_waiting());
        assert_eq!(q.items[1].error.as_deref(), Some("could not decode"));
        q.clear_finished();
        assert!(q.is_empty());
    }

    #[test]
    fn hold_remove_cancel_and_recover() {
        let mut q = Queue { state: QueueState::Running, ..Default::default() };
        q.add(&["A".into(), "B".into(), "C".into()]);
        assert!(q.hold("A", true));
        assert_eq!(q.take_next().as_deref(), Some("B"), "a held file is skipped");
        assert!(q.hold("A", false));
        // A crash while B runs: on open, B waits again and the queue waits for Resume.
        let mut back: Queue = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
        assert!(back.recover());
        assert_eq!(back.state, QueueState::Paused);
        assert!(back.interrupted);
        assert_eq!(ids(&back)[1], ("B".into(), ItemState::Waiting));
        // Holding the running file stops it; finish then leaves it held.
        q.hold("B", true);
        q.finish("B", Err("cancelled".into()), true);
        assert_eq!(q.items[1].state, ItemState::Held);
        assert!(q.remove("C"));
        q.cancel_all();
        assert!(q.is_empty() && q.state == QueueState::Idle);
    }
}
