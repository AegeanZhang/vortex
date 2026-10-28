//! 只选择当前可见的控件内容；布局或内容变化使旧选区失效。
use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Modifier,
    text::Span,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Region {
    area: Rect,
    rows: Vec<Vec<String>>,
}

impl Region {
    fn column_start(&self, row: usize, column: usize) -> usize {
        let mut x = 0;
        while x < self.area.width as usize {
            let width = Span::raw(&self.rows[row][x]).width().max(1);
            if column < x + width {
                return x;
            }
            x += width;
        }
        column
    }
}

#[derive(Debug, Clone, Copy)]
struct Range {
    region: usize,
    anchor: (usize, usize),
    end: (usize, usize),
    dragging: bool,
}

#[derive(Debug, Default)]
pub(crate) struct TextSelection {
    regions: Vec<Region>,
    range: Option<Range>,
}

impl TextSelection {
    pub(crate) fn is_active(&self) -> bool {
        self.range.is_some()
    }

    pub(crate) fn clear(&mut self) {
        self.range = None;
    }

    pub(crate) fn capture(&mut self, buffer: &Buffer, areas: &[Rect]) {
        let regions: Vec<_> = areas
            .iter()
            .map(|area| {
                let area = area.intersection(buffer.area);
                Region {
                    area,
                    rows: (area.y..area.bottom())
                        .map(|y| {
                            (area.x..area.right())
                                .map(|x| buffer[(x, y)].symbol().to_owned())
                                .collect()
                        })
                        .collect(),
                }
            })
            .collect();
        if self
            .range
            .is_some_and(|range| self.regions.get(range.region) != regions.get(range.region))
        {
            self.clear();
        }
        self.regions = regions;
    }

    pub(crate) fn mouse(&mut self, event: MouseEvent) {
        let position = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(_) => {
                self.range = self.regions.iter().enumerate().find_map(|(index, region)| {
                    if !region.area.contains(position) {
                        return None;
                    }
                    let row = (event.row - region.area.y) as usize;
                    let column = region.column_start(row, (event.column - region.area.x) as usize);
                    Some(Range {
                        region: index,
                        anchor: (row, column),
                        end: (row, column),
                        dragging: true,
                    })
                });
            }
            MouseEventKind::Drag(_) | MouseEventKind::Up(_) => {
                if let Some(range) = &mut self.range
                    && range.dragging
                {
                    let region = &self.regions[range.region];
                    let row =
                        event.row.clamp(region.area.y, region.area.bottom() - 1) - region.area.y;
                    let column =
                        event.column.clamp(region.area.x, region.area.right() - 1) - region.area.x;
                    range.end = (
                        row as usize,
                        region.column_start(row as usize, column as usize),
                    );
                    if matches!(event.kind, MouseEventKind::Up(_)) {
                        range.dragging = false;
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn text(&self) -> Option<String> {
        let range = self.range?;
        let region = &self.regions[range.region];
        let (start, end) = if range.anchor <= range.end {
            (range.anchor, range.end)
        } else {
            (range.end, range.anchor)
        };
        let mut lines = Vec::new();
        for row in start.0..=end.0 {
            let left = if row == start.0 { start.1 } else { 0 };
            let right = if row == end.0 {
                end.1
            } else {
                region.area.width as usize - 1
            };
            let mut line = String::new();
            let mut x = left;
            while x <= right {
                let symbol = &region.rows[row][x];
                line.push_str(symbol);
                x += Span::raw(symbol).width().max(1);
            }
            lines.push(line.trim_end_matches(' ').to_owned());
        }
        let text = lines.join("\n");
        (!text.is_empty()).then_some(text)
    }

    pub(crate) fn highlight(&self, buffer: &mut Buffer) {
        let Some(range) = self.range else {
            return;
        };
        let region = &self.regions[range.region];
        let (start, end) = if range.anchor <= range.end {
            (range.anchor, range.end)
        } else {
            (range.end, range.anchor)
        };
        for row in start.0..=end.0 {
            let left = if row == start.0 { start.1 } else { 0 };
            let right = if row == end.0 {
                end.1 + Span::raw(&region.rows[row][end.1]).width().max(1) - 1
            } else {
                region.area.width as usize - 1
            };
            for column in left..=right.min(region.area.width as usize - 1) {
                let cell = &mut buffer[(region.area.x + column as u16, region.area.y + row as u16)];
                // 反色属性不依赖调色板；NO_COLOR 下也能看见选区。
                cell.modifier.insert(Modifier::REVERSED);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseButton};
    use ratatui::{
        text::Text,
        widgets::{Paragraph, Widget},
    };

    fn event(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }
    fn drag(selection: &mut TextSelection, start: (u16, u16), end: (u16, u16)) {
        selection.mouse(event(
            MouseEventKind::Down(MouseButton::Left),
            start.0,
            start.1,
        ));
        selection.mouse(event(MouseEventKind::Drag(MouseButton::Left), end.0, end.1));
        selection.mouse(event(MouseEventKind::Up(MouseButton::Left), end.0, end.1));
    }
    fn fixture() -> (Buffer, TextSelection) {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 5));
        Paragraph::new(Text::from("abc中文 xyz\n  code();\ne\u{301}😀 end"))
            .render(Rect::new(1, 1, 14, 3), &mut buffer);
        let mut selection = TextSelection::default();
        selection.capture(&buffer, &[Rect::new(1, 1, 14, 3)]);
        (buffer, selection)
    }
    #[test]
    fn updates_in_other_region_preserve_active_prompt_selection() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 12, 4));
        Paragraph::new("first").render(Rect::new(0, 0, 12, 1), &mut buffer);
        Paragraph::new("prompt").render(Rect::new(0, 3, 12, 1), &mut buffer);
        let regions = [Rect::new(0, 0, 12, 1), Rect::new(0, 3, 12, 1)];
        let mut selection = TextSelection::default();
        selection.capture(&buffer, &regions);
        drag(&mut selection, (0, 3), (5, 3));
        buffer[(0, 0)].set_symbol("X");
        selection.capture(&buffer, &regions);
        assert_eq!(selection.text().as_deref(), Some("prompt"));
    }

    #[test]
    fn forward_and_reverse_selection_preserve_wide_characters() {
        let (_, mut selection) = fixture();
        drag(&mut selection, (4, 1), (7, 1)); // 从“中”首格到“文”占位格。
        assert_eq!(selection.text().as_deref(), Some("中文"));
        drag(&mut selection, (7, 1), (4, 1));
        assert_eq!(selection.text().as_deref(), Some("中文"));
    }
    #[test]
    fn copy_joins_visual_lines_preserving_indent_combining_marks_and_emoji() {
        let (_, mut selection) = fixture();
        drag(&mut selection, (1, 2), (3, 3));
        assert_eq!(selection.text().as_deref(), Some("  code();\ne\u{301}😀"));
    }
    #[test]
    fn selection_excludes_chrome_and_clamps_drag_to_original_region() {
        let (_, mut selection) = fixture();
        drag(&mut selection, (1, 1), (19, 4));
        assert_eq!(
            selection.text().as_deref(),
            Some("abc中文 xyz\n  code();\ne\u{301}😀 end")
        );
        drag(&mut selection, (0, 0), (5, 1));
        assert!(selection.text().is_none());
    }
    #[test]
    fn content_or_geometry_changes_invalidate_selection_but_repaint_does_not() {
        let (mut buffer, mut selection) = fixture();
        let areas = [Rect::new(1, 1, 14, 3)];
        drag(&mut selection, (1, 1), (3, 1));
        selection.capture(&buffer, &areas);
        assert_eq!(selection.text().as_deref(), Some("abc"));
        selection.highlight(&mut buffer);
        assert!(buffer[(1, 1)].modifier.contains(Modifier::REVERSED));
        buffer[(1, 1)].set_symbol("z");
        selection.capture(&buffer, &areas);
        assert!(selection.text().is_none());
        drag(&mut selection, (1, 1), (3, 1));
        selection.capture(&buffer, &[Rect::new(1, 1, 13, 3)]);
        assert!(selection.text().is_none());
    }
    #[test]
    fn empty_regions_and_blank_selection_are_safe() {
        let buffer = Buffer::empty(Rect::new(0, 0, 2, 2));
        let mut selection = TextSelection::default();
        selection.capture(&buffer, &[Rect::default()]);
        drag(&mut selection, (0, 0), (1, 1));
        assert!(selection.text().is_none());
        selection.capture(&buffer, &[buffer.area]);
        drag(&mut selection, (0, 0), (1, 1));
        assert_eq!(selection.text().as_deref(), Some("\n"));
    }
}
