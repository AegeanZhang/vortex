use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
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

#[derive(Debug, Default)]
pub(crate) struct Transcript {
    entries: Vec<TranscriptEntry>,
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

        Self { entries }
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines = Vec::new();

        if self.entries.is_empty() {
            lines.push(Line::from("Type a prompt and press Enter."));
        } else {
            for (index, entry) in self.entries.iter().enumerate() {
                if index > 0 {
                    lines.push(Line::default());
                }

                lines.push(Line::from(format!("{}:", entry.role.label())));

                for content_line in entry.content.lines() {
                    lines.push(Line::from(content_line));
                }
            }
        }

        let transcript = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Transcript"))
            .wrap(Wrap { trim: false });

        frame.render_widget(transcript, area);
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
