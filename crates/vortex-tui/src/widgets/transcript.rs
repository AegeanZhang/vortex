//! Transcript 展示组件。
//!
//! 负责将 Session 消息投影为对话条目，并渲染用户、Assistant 和错误消息。

use crate::markdown;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
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
    reasoning: String,
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
            reasoning: String::new(),
        });
    }

    pub(crate) fn start_assistant(&mut self) {
        self.entries.push(TranscriptEntry {
            role: TranscriptRole::Assistant,
            content: String::new(),
            reasoning: String::new(),
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
            reasoning: String::new(),
        });
    }

    pub(crate) fn start_model_step(&mut self) {
        let reuse_empty = self.entries.last().is_some_and(|entry| {
            entry.role == TranscriptRole::Assistant
                && entry.content.is_empty()
                && entry.reasoning.is_empty()
        });
        if !reuse_empty {
            self.start_assistant();
        }
    }

    pub(crate) fn append_reasoning_delta(&mut self, delta: String) {
        if delta.is_empty() {
            return;
        }
        let has_assistant = self
            .entries
            .last()
            .is_some_and(|entry| entry.role == TranscriptRole::Assistant);
        if !has_assistant {
            self.start_assistant();
        }
        if let Some(entry) = self.entries.last_mut() {
            entry.reasoning.push_str(&delta);
        }
    }

    fn push_error(&mut self, message: String) {
        self.entries.push(TranscriptEntry {
            role: TranscriptRole::Error,
            content: message,
            reasoning: String::new(),
        });
    }

    pub(crate) fn fail_assistant(&mut self, message: String) {
        let should_remove_empty_assistant = self.entries.last().is_some_and(|entry| {
            entry.role == TranscriptRole::Assistant
                && entry.content.is_empty()
                && entry.reasoning.is_empty()
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
                    reasoning: String::new(),
                })
            })
            .collect();

        Self {
            entries,
            ..Default::default()
        }
    }

    fn render_lines(entries: &[TranscriptEntry]) -> Vec<Line<'_>> {
        if entries.is_empty() {
            return vec![Line::from("Type a prompt and press Enter.")];
        }

        let mut lines = Vec::new();

        for (index, entry) in entries.iter().enumerate() {
            if index > 0 {
                lines.push(Line::default());
            }

            lines.push(Line::from(format!("{}:", entry.role.label())));

            match entry.role {
                TranscriptRole::Assistant => {
                    if !entry.reasoning.is_empty() {
                        let style = Style::default().fg(Color::DarkGray);

                        lines.push(Line::styled("Thinking:", style));
                        lines.extend(
                            entry
                                .reasoning
                                .lines()
                                .map(|line| Line::styled(line, style)),
                        );

                        if !entry.content.is_empty() {
                            lines.push(Line::default());
                        }
                    }

                    lines.extend(markdown::render(&entry.content));
                }
                TranscriptRole::User | TranscriptRole::Error => {
                    lines.extend(entry.content.lines().map(Line::from));
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

        self.scroll.offset = self.scroll.offset.saturating_add(lines).min(max_offset);

        self.scroll.follow_tail = self.scroll.offset == max_offset;
    }

    pub(crate) fn page_up(&mut self) {
        self.scroll_up(self.page_size());
    }

    pub(crate) fn page_down(&mut self) {
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
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    fn draw(transcript: &mut Transcript, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| transcript.render(frame, frame.area()))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn screen(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn reasoning_survives_failure_and_model_steps_are_separate() {
        let mut transcript = Transcript::default();
        transcript.start_assistant();
        transcript.start_model_step();
        assert_eq!(transcript.entries.len(), 1);
        transcript.append_reasoning_delta("第一步思考".into());
        transcript.start_model_step();
        assert_eq!(transcript.entries.len(), 2);
        transcript.append_reasoning_delta("第二步思考".into());
        transcript.fail_assistant("中断".into());
        assert_eq!(transcript.entries.len(), 3);
        assert_eq!(transcript.entries[0].reasoning, "第一步思考");
        assert_eq!(transcript.entries[1].reasoning, "第二步思考");
        assert!(
            transcript.entries[..2]
                .iter()
                .all(|entry| entry.content.is_empty())
        );
        assert_eq!(transcript.entries[2].role, TranscriptRole::Error);
    }

    #[test]
    fn failure_removes_only_a_completely_empty_assistant() {
        for (reasoning, content, expected_len) in
            [("", "", 1), ("R", "", 2), ("", "A", 2), ("R", "A", 2)]
        {
            let mut transcript = Transcript::default();
            transcript.start_assistant();
            transcript.append_reasoning_delta(reasoning.into());
            transcript.append_assistant_delta(content.into());
            transcript.fail_assistant("fixture failure".into());
            assert_eq!(transcript.entries.len(), expected_len);
            assert_eq!(
                transcript.entries.last().unwrap().role,
                TranscriptRole::Error
            );
            if expected_len == 2 {
                assert_eq!(transcript.entries[0].reasoning, reasoning);
                assert_eq!(transcript.entries[0].content, content);
            }
        }
    }

    #[test]
    fn reasoning_is_plain_dimmed_text_and_answer_is_markdown() {
        let mut transcript = Transcript::default();
        transcript.start_model_step();
        transcript.append_reasoning_delta("**literal reasoning**".into());
        transcript.append_assistant_delta("**answer**".into());
        let buffer = draw(&mut transcript, 60, 12);
        let rendered = screen(&buffer);
        assert!(rendered.contains("Thinking:"));
        assert!(rendered.contains("**literal reasoning**"));
        assert!(rendered.contains("answer"));
        assert!(!rendered.contains("**answer**"));
        let reasoning_row = (0..12).find(|&y| buffer[(1, y)].symbol() == "*").unwrap();
        assert_eq!(buffer[(1, reasoning_row)].fg, Color::DarkGray);
        let answer_row = (0..12).find(|&y| buffer[(1, y)].symbol() == "a").unwrap();
        assert!(answer_row > reasoning_row + 1);
    }

    #[test]
    fn empty_reasoning_does_not_create_an_entry_or_a_label() {
        let mut transcript = Transcript::default();
        transcript.append_reasoning_delta(String::new());
        assert!(transcript.entries.is_empty());
        transcript.append_assistant_delta("answer".into());
        assert!(!screen(&draw(&mut transcript, 40, 8)).contains("Thinking:"));
    }

    #[test]
    fn long_chinese_reasoning_scrolls_without_forcing_the_reader_to_the_tail() {
        let mut transcript = Transcript::default();
        transcript.append_reasoning_delta(format!(
            "BEGIN\n{}\nEND",
            "这是一段用于验证中文换行与滚动的思考内容。".repeat(40)
        ));
        let bottom = screen(&draw(&mut transcript, 32, 8));
        assert!(transcript.scroll.content_height > transcript.scroll.viewport_height);
        assert_eq!(transcript.scroll.offset, transcript.max_offset());
        assert!(bottom.contains("END"));
        transcript.page_up();
        assert!(!transcript.scroll.follow_tail);
        let before = screen(&draw(&mut transcript, 32, 8));
        let offset = transcript.scroll.offset;
        transcript.append_reasoning_delta("\nNEW-END".into());
        assert_eq!(screen(&draw(&mut transcript, 32, 8)), before);
        assert_eq!(transcript.scroll.offset, offset);

        let old_height = transcript.scroll.content_height;
        draw(&mut transcript, 20, 10);
        assert!(transcript.scroll.content_height > old_height);
        assert!(transcript.scroll.offset <= transcript.max_offset());
        assert!(!transcript.scroll.follow_tail);
        transcript.scroll_to_top();
        assert!(screen(&draw(&mut transcript, 20, 10)).contains("BEGIN"));
        transcript.scroll_to_bottom();
        assert!(screen(&draw(&mut transcript, 20, 10)).contains("NEW-END"));
        assert_eq!(transcript.scroll.offset, transcript.max_offset());
    }

    #[test]
    fn thinking_and_multiple_steps_render_in_tiny_viewports_and_after_resize() {
        let mut transcript = Transcript::default();
        transcript.start_assistant();
        transcript.append_reasoning_delta("第一步思考".into());
        transcript.start_model_step();
        transcript.append_reasoning_delta("第二步思考".into());
        transcript.append_assistant_delta("最终正文".into());
        for (width, height) in [(0, 0), (1, 1), (3, 3), (12, 4), (60, 20)] {
            draw(&mut transcript, width, height);
            assert!(transcript.scroll.offset <= transcript.max_offset());
        }
        let rendered = screen(&draw(&mut transcript, 60, 20));
        assert_eq!(rendered.matches("Thinking:").count(), 2);
        let compact = rendered.replace(' ', "");
        assert!(compact.contains("第一步思考"));
        assert!(compact.contains("第二步思考"));
        assert!(compact.contains("最终正文"));
    }

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
