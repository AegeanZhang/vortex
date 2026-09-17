//! Transcript 展示组件。
//!
//! 负责将 Session 消息投影为对话条目，并渲染用户、Assistant 和错误消息。
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};

use vortex_core::{MessageRole, SessionMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranscriptRole {
    User,
    Assistant,
    Error,
}

impl TranscriptRole {
    fn label(self) -> &'static str {
        match self {
            Self::User => "You",
            Self::Assistant => "Assistant",
            Self::Error => "Error",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct TranscriptEntry {
    //label: &'static str,
    role: TranscriptRole,
    content: String,
}

#[derive(Debug)]
struct TranscriptScroll {
    offset: usize,
    follow_tail: bool,
    content_height: usize,
    viewport_height: usize,
}

impl Default for TranscriptScroll {
    fn default() -> Self {
        Self {
            offset: 0,
            follow_tail: true,
            content_height: 0,
            viewport_height: 0,
        }
    }
}

impl TranscriptScroll {
    fn sync_viewport(&mut self, content_height: usize, viewport_height: usize) {
        self.content_height = content_height;
        self.viewport_height = viewport_height;

        let max_offset = content_height.saturating_sub(viewport_height);

        if self.follow_tail {
            self.offset = max_offset;
        } else {
            self.offset = self.offset.min(max_offset);
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Transcript {
    entries: Vec<TranscriptEntry>,
    scroll: TranscriptScroll,
}

impl Transcript {
    pub(crate) fn push_user(&mut self, content: String) {
        self.entries.push(TranscriptEntry {
            role: TranscriptRole::User,
            content,
        });
    }

    pub(crate) fn start_assistant(&mut self) {
        self.entries.push(TranscriptEntry {
            role: TranscriptRole::Assistant,
            content: String::new(),
        });
    }

    pub(crate) fn append_assistant_delta(&mut self, delta: String) {
        if let Some(entry) = self.entries.last_mut()
            && entry.role == TranscriptRole::Assistant
        {
            entry.content.push_str(&delta);
            return;
        }

        self.entries.push(TranscriptEntry {
            role: TranscriptRole::Assistant,
            content: delta,
        });
    }

    fn push_error(&mut self, message: String) {
        self.entries.push(TranscriptEntry {
            role: TranscriptRole::Error,
            content: message,
        });
    }

    pub(crate) fn fail_assistant(&mut self, message: String) {
        let should_remove_empty_assistant = self.entries.last().is_some_and(|entry| {
            entry.role == TranscriptRole::Assistant && entry.content.is_empty()
        });

        if should_remove_empty_assistant {
            self.entries.pop();
        }

        self.push_error(message);
    }

    pub(crate) fn from_messages(messages: Vec<SessionMessage>) -> Self {
        let entries = messages
            .into_iter()
            .filter_map(|message| {
                let role = match message.role {
                    MessageRole::User => TranscriptRole::User,
                    MessageRole::Assistant => TranscriptRole::Assistant,
                    MessageRole::System => return None,
                };

                Some(TranscriptEntry {
                    role,
                    content: message.content,
                })
            })
            .collect();

        Self {
            entries,
            ..Default::default()
        }
    }

    fn render_lines(entries: &[TranscriptEntry]) -> Vec<Line<'_>> {
        let mut lines = Vec::new();

        if entries.is_empty() {
            lines.push(Line::from("Type a prompt and press Enter."));
        } else {
            for (index, entry) in entries.iter().enumerate() {
                if index > 0 {
                    lines.push(Line::default());
                }

                lines.push(Line::from(format!("{}:", entry.role.label())));

                for content_line in entry.content.lines() {
                    lines.push(Line::from(content_line));
                }
            }
        }

        lines
    }

    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Transcript");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width <= 1 || inner.height == 0 {
            self.scroll.sync_viewport(0, 0);
            return;
        }

        let content_area = Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        };

        let lines = Self::render_lines(&self.entries);
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });

        let content_height = paragraph.line_count(content_area.width);
        let viewport_height = content_area.height as usize;

        self.scroll.sync_viewport(content_height, viewport_height);

        let vertical_offset = u16::try_from(self.scroll.offset).unwrap_or(u16::MAX);

        frame.render_widget(paragraph.scroll((vertical_offset, 0)), content_area);

        if content_height > viewport_height {
            /*
            let mut scrollbar_state = ScrollbarState::new(content_height)
                .position(self.scroll.offset)
                .viewport_content_length(viewport_height);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);

            frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
            */

            let scroll_positions = self.max_offset().saturating_add(1);

            let mut scrollbar_state = ScrollbarState::new(scroll_positions)
                .position(self.scroll.offset)
                .viewport_content_length(viewport_height);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);

            frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
        }
    }

    fn max_offset(&self) -> usize {
        self.scroll
            .content_height
            .saturating_sub(self.scroll.viewport_height)
    }

    fn page_size(&self) -> usize {
        self.scroll.viewport_height.saturating_sub(1).max(1)
    }

    pub(crate) fn scroll_up(&mut self, lines: usize) {
        if lines == 0 || self.max_offset() == 0 {
            return;
        }

        self.scroll.offset = self.scroll.offset.saturating_sub(lines);
        self.scroll.follow_tail = false;
    }

    pub(crate) fn scroll_down(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }

        let max_offset = self.max_offset();

        self.scroll.offset = self
            .scroll
            .offset
            .saturating_add(lines)
            .min(max_offset);

        self.scroll.follow_tail = self.scroll.offset == max_offset;
    }

    pub(crate) fn page_up(&mut self) {
        /* 
        let max_offset = self.max_offset();

        if max_offset == 0 {
            return;
        }

        self.scroll.offset = self.scroll.offset.saturating_sub(self.page_size());
        self.scroll.follow_tail = false;
        */
        self.scroll_up(self.page_size());
    }

    pub(crate) fn page_down(&mut self) {
        /*let max_offset = self.max_offset();

        self.scroll.offset = self
            .scroll
            .offset
            .saturating_add(self.page_size())
            .min(max_offset);

        self.scroll.follow_tail = self.scroll.offset == max_offset;*/

        self.scroll_down(self.page_size());
    }

    pub(crate) fn scroll_to_top(&mut self) {
        self.scroll.offset = 0;
        self.scroll.follow_tail = self.max_offset() == 0;
    }

    pub(crate) fn scroll_to_bottom(&mut self) {
        self.scroll.offset = self.max_offset();
        self.scroll.follow_tail = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_assistant_deltas_to_same_entry() {
        let mut transcript = Transcript::default();

        transcript.start_assistant();
        transcript.append_assistant_delta("Hello".to_string());
        transcript.append_assistant_delta(" world".to_string());

        assert_eq!(transcript.entries.len(), 1);
        assert_eq!(transcript.entries[0].content, "Hello world");
    }

    #[test]
    fn error_is_a_separate_entry() {
        let mut transcript = Transcript::default();

        transcript.push_error("request failed".to_string());

        assert_eq!(transcript.entries[0].role, TranscriptRole::Error);
    }
}
