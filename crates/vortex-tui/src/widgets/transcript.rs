use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

#[derive(Debug)]
struct TranscriptEntry {
    label: &'static str,
    content: String,
}

#[derive(Debug, Default)]
pub(crate) struct Transcript {
    entries: Vec<TranscriptEntry>,
}

impl Transcript {
    pub(crate) fn push_user(&mut self, content: String) {
        self.entries.push(TranscriptEntry {
            label: "You",
            content,
        });
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

                lines.push(Line::from(format!("{}:", entry.label)));

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
