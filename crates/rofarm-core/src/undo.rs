use crate::events::DomainEvent;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub struct UndoEntry {
    pub label: String,
    pub undo_events: Vec<DomainEvent>,
    pub redo_events: Vec<DomainEvent>,
}

#[derive(Debug)]
pub struct UndoStack {
    undo: VecDeque<UndoEntry>,
    redo: Vec<UndoEntry>,
    limit: usize,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(50)
    }
}

impl UndoStack {
    pub fn new(limit: usize) -> Self {
        Self { undo: VecDeque::new(), redo: Vec::new(), limit: limit.max(1) }
    }

    pub fn push(&mut self, entry: UndoEntry) {
        self.redo.clear();
        self.undo.push_back(entry);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
    }

    pub fn undo(&mut self) -> Option<UndoEntry> {
        let entry = self.undo.pop_back()?;
        self.redo.push(entry.clone());
        Some(entry)
    }

    pub fn redo(&mut self) -> Option<UndoEntry> {
        let entry = self.redo.pop()?;
        self.undo.push_back(entry.clone());
        Some(entry)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.back().map(|e| e.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|e| e.label.as_str())
    }

    pub fn len(&self) -> usize {
        self.undo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Account;
    use crate::events::DomainEvent;
    use crate::ids::builtin;

    fn entry(label: &str) -> UndoEntry {
        let a = Account::new(label, builtin::idle());
        UndoEntry {
            label: label.into(),
            undo_events: vec![DomainEvent::AccountRemoved { account_id: a.id, snapshot: a.clone() }],
            redo_events: vec![DomainEvent::AccountAdded { account: a }],
        }
    }

    #[test]
    fn undo_redo_round_trip_and_limit() {
        let mut stack = UndoStack::new(2);
        stack.push(entry("one"));
        stack.push(entry("two"));
        stack.push(entry("three"));
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.undo_label(), Some("three"));
        let undone = stack.undo().unwrap();
        assert_eq!(undone.label, "three");
        assert_eq!(stack.redo_label(), Some("three"));
        assert_eq!(stack.undo_label(), Some("two"));
        let redone = stack.redo().unwrap();
        assert_eq!(redone.label, "three");
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn pushing_invalidates_redo() {
        let mut stack = UndoStack::new(10);
        stack.push(entry("a"));
        stack.undo();
        assert!(stack.can_redo());
        stack.push(entry("b"));
        assert!(!stack.can_redo());
    }
}
