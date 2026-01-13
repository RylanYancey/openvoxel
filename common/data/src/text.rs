//! Utilities for recording text input.

use std::collections::VecDeque;

use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyCode, KeyboardInput},
    },
    prelude::*,
};

/// Helper struct for working with single-line text input.
#[derive(Default)]
pub struct TextRecorder {
    buffer: String,
    cursor: usize,
}

impl TextRecorder {
    /// Get the number of characters in the buffer.
    pub fn len(&self) -> usize {
        // Count the number of characters, not the number of bytes.
        self.buffer.chars().count()
    }

    /// Get the number of bytes in the buffer.
    pub fn size(&self) -> usize {
        self.buffer.len()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    /// Overwrise existing text with contents of a new string.
    /// Sets the cursor to 0.
    pub fn set(&mut self, text: impl Into<String>) {
        self.buffer = text.into();
        self.cursor = 0;
    }

    /// Insert a string at the cursor and advance the cursor by the length of the string.
    pub fn insert_str(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref();
        self.buffer.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    /// Insert a character at the cursor and advances the cursor by 1.
    pub fn insert(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete the character just before the cursor.
    /// Decrements the cursor, then deletes.
    pub fn backspace(&mut self) {
        if let Some(cursor) = self.try_prev() {
            self.buffer.remove(cursor);
        }
    }

    /// Delete the character at the cursor.
    /// Does not change the cursor. Does nothing
    /// if the cursor is at the end.
    pub fn delete(&mut self) {
        if !self.is_at_end() {
            self.buffer.remove(self.cursor);
        }
    }

    /// Check whether the cursor is at the end of the buffer.
    pub fn is_at_end(&self) -> bool {
        self.cursor < self.buffer.len()
    }

    /// Advance the cursor and get the new value if it was able to move.
    pub fn try_next(&mut self) -> Option<usize> {
        if self.cursor < self.buffer.len() {
            self.cursor += 1;
            Some(self.cursor)
        } else {
            None
        }
    }

    /// Decrement cursor and get new value if decrement was successful.
    pub fn try_prev(&mut self) -> Option<usize> {
        if self.cursor != 0 {
            self.cursor -= 1;
            Some(self.cursor)
        } else {
            None
        }
    }

    /// Advance cursor to next index, if it is available.
    pub fn cursor_next(&mut self) {
        self.try_next();
    }

    /// Go to the previous index, if available.
    pub fn cursor_prev(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Set the cursor to the end of the buffer.
    pub fn go_to_end(&mut self) {
        self.cursor = self.buffer.len()
    }

    /// Set the cursor to the start of the buffer.
    pub fn go_to_home(&mut self) {
        self.cursor = 0;
    }

    pub fn read(&self) -> &str {
        &self.buffer
    }

    /// Returns a SpecialKey if the input has implications or has no effect.
    pub fn update(&mut self, ev: &KeyboardInput) -> Option<SpecialKey> {
        if ev.state == ButtonState::Released {
            return None;
        }

        match &ev.logical_key {
            Key::Character(c) => self.insert_str(c),
            _ => match ev.key_code {
                KeyCode::Space => self.insert(' '),
                KeyCode::Delete => self.delete(),
                KeyCode::Backspace => self.backspace(),
                KeyCode::ArrowRight => {
                    if self.try_next().is_none() {
                        return Some(SpecialKey::Autocomplete);
                    }
                }
                KeyCode::ArrowLeft => self.cursor_prev(),
                KeyCode::Home => self.go_to_home(),
                KeyCode::End => self.go_to_end(),
                KeyCode::Enter => return Some(SpecialKey::Submit),
                KeyCode::Tab => return Some(SpecialKey::Autocomplete),
                KeyCode::ArrowUp => return Some(SpecialKey::HistoryUp),
                KeyCode::ArrowDown => return Some(SpecialKey::HistoryDown),
                _ => return Some(SpecialKey::NoEffect),
            },
        }

        None
    }

    /// Consume the buffer.
    pub fn submit(&mut self) -> String {
        let ret = self.buffer.clone();
        self.buffer.clear();
        self.cursor = 0;
        ret
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum SpecialKey {
    /// User submitted the buffer.
    Submit,

    /// User requested autocompletion.
    /// Happens when the user pressed TAB (anytime),
    /// or when the cursor is at the end of the
    /// buffer and ArrowRight is pressed.
    Autocomplete,

    /// User pressed ArrowUp.
    HistoryUp,

    /// User pressed ArrowDown.
    HistoryDown,

    /// The key was not a character or special.
    NoEffect,
}

/// Helper struct for keeping track of recorded text input.
#[derive(Default)]
pub struct TextHistory {
    /// Back of the buffer (low index) is newer messages.
    history: VecDeque<String>,

    /// Currently selected item in history.
    cursor: usize,

    /// Max number of items to keep in the history.
    limit: Option<usize>,
}

impl TextHistory {
    /// Create a new TextHistory with a fixed limit.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            history: VecDeque::new(),
            cursor: 0,
            limit: Some(limit),
        }
    }

    /// Clear all elements from the history.
    pub fn clear(&mut self) {
        self.history.clear();
        self.cursor = 0;
    }

    /// Move the cursor to the newest item in the history.
    pub fn go_to_start(&mut self) {
        self.cursor = 0;
    }

    /// Move the cursor to the end of the history.
    pub fn go_to_end(&mut self) {
        self.cursor = self.history.len();
    }

    /// Push a new item onto the back of the stack, and
    /// reset the cursor to the newest item.
    pub fn push(&mut self, item: String) {
        self.cursor = 0;
        self.history.push_back(item);
        if let Some(limit) = self.limit {
            if self.history.len() > limit {
                self.history.pop_front();
            }
        }
    }

    /// Get the item at the cursor.
    /// Returns nothing if the history is empty.
    /// If the cursor is `history.len()`, then the last element is returned.
    pub fn curr(&self) -> Option<&str> {
        self.history
            .get(self.cursor)
            .or_else(|| self.history.front())
            .map(|s| s.as_str())
    }

    /// Get the next oldest item, if not already at the end of the stack.
    pub fn prev(&mut self) -> Option<&str> {
        if self.cursor < self.history.len() {
            let item = &self.history[self.cursor];
            self.cursor += 1;
            Some(item)
        } else {
            None
        }
    }

    /// Get the next newest item, if not already at the newest.
    pub fn next(&mut self) -> Option<&str> {
        if self.cursor != 0 {
            let item = &self.history[self.cursor];
            self.cursor -= 1;
            Some(item)
        } else {
            None
        }
    }
}
