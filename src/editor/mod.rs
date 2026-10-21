//! The code editor panel.
//!
//! Built on egui's `TextEdit` with a custom layouter for syntax highlighting.
//! Around it the editor adds a gutter (line numbers, breakpoints, the debug
//! position), current/error/debug line backgrounds, a line-length ruler, a
//! search bar and the completion popup.

pub mod highlight;
pub mod text_ops;

use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use egui::text::{CCursor, CCursorRange};
use egui::{
    Align, Align2, Color32, FontId, Galley, Key, Modifiers, Pos2, Rect, Sense, Shape, Stroke,
    TextBuffer, TextEdit, Vec2,
};

use crate::document::Document;
use crate::worker::protocol::CompletionItem;
use highlight::SyntaxColors;
use text_ops::Selection;

/// Most search matches highlighted at once.
const MAX_HIGHLIGHTED_MATCHES: usize = 2000;
const COMPLETION_ROWS: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub struct EditorColors {
    pub syntax: SyntaxColors,
    pub gutter_text: Color32,
    pub gutter_text_current: Color32,
    pub current_line: Color32,
    pub ruler: Color32,
    pub breakpoint: Color32,
    pub debug_line: Color32,
    pub debug_marker: Color32,
    pub error_line: Color32,
    pub error_text: Color32,
    pub search_match: Color32,
    pub search_current: Color32,
}

pub struct EditorOptions<'a> {
    pub font_size: f32,
    pub line_wrap: bool,
    /// Ruler column; 0 hides it.
    pub ruler: usize,
    pub highlight_current_line: bool,
    pub auto_indent: bool,
    pub colors: &'a EditorColors,
}

/// Where the editor wants completions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletionRequest {
    /// 1-based.
    pub line: usize,
    /// 0-based, in characters.
    pub column: usize,
}

#[derive(Default)]
pub struct EditorOutput {
    pub changed: bool,
    pub completion_request: Option<CompletionRequest>,
}

#[derive(Default)]
struct Search {
    open: bool,
    query: String,
    match_case: bool,
    matches: Vec<Range<usize>>,
    current: usize,
    focus_query: bool,
    computed_for: u64,
}

#[derive(Default)]
struct Completion {
    items: Vec<CompletionItem>,
    selected: usize,
    /// Cursor position (byte) when the completion was requested.
    anchor: usize,
    open: bool,
    scroll_to_selected: bool,
}

/// The highlighted layout job of the last text. Galleys themselves are not
/// kept: egui rebuilds its font atlas now and then, which invalidates old
/// galleys, while its own galley cache handles that.
#[derive(Default)]
struct LayoutCache {
    key: u64,
    job: Option<egui::text::LayoutJob>,
    /// Width of the last laid out text, for horizontal scrolling.
    width: f32,
}

pub struct CodeEditor {
    /// 1-based line numbers.
    pub breakpoints: BTreeSet<usize>,
    pub debug_line: Option<usize>,
    pub error_line: Option<usize>,
    text_id: egui::Id,
    /// Last known selection, in bytes.
    selection: Selection,
    pending_selection: Option<Selection>,
    pending_scroll_line: Option<usize>,
    pending_focus: bool,
    completion_wanted: bool,
    /// Cursor position (byte) of the last completion request; the worker's
    /// answer completes the text from there.
    completion_anchor: usize,
    completion: Completion,
    calltip: Option<String>,
    search: Search,
    layout: LayoutCache,
}

impl Default for CodeEditor {
    fn default() -> Self {
        Self {
            breakpoints: BTreeSet::new(),
            debug_line: None,
            error_line: None,
            text_id: egui::Id::new("code_editor_text"),
            selection: Selection::caret(0),
            pending_selection: None,
            pending_scroll_line: None,
            pending_focus: false,
            completion_wanted: false,
            completion_anchor: 0,
            completion: Completion::default(),
            calltip: None,
            search: Search::default(),
            layout: LayoutCache::default(),
        }
    }
}

impl CodeEditor {
    /// 1-based line and column of the cursor, for the status bar.
    pub fn cursor_position(&self, text: &str) -> (usize, usize) {
        let (line, column) = text_ops::line_column(text, self.selection.cursor.min(text.len()));
        (line + 1, column + 1)
    }

    /// Forgets per-document state when another file is loaded.
    pub fn reset(&mut self) {
        self.breakpoints.clear();
        self.debug_line = None;
        self.error_line = None;
        self.close_popups();
        self.pending_selection = Some(Selection::caret(0));
        self.pending_scroll_line = Some(1);
    }

    pub fn go_to_line(&mut self, text: &str, line: usize) {
        let start = text_ops::line_start(text, line.saturating_sub(1));
        self.pending_selection = Some(Selection::caret(start));
        self.pending_scroll_line = Some(line);
        self.pending_focus = true;
    }

    pub fn set_debug_line(&mut self, line: Option<usize>) {
        self.debug_line = line;
        if line.is_some() {
            self.pending_scroll_line = line;
        }
    }

    pub fn toggle_breakpoint(&mut self, line: usize) {
        if !self.breakpoints.remove(&line) {
            self.breakpoints.insert(line);
        }
    }

    pub fn toggle_comment(&mut self, document: &mut Document) {
        let edited =
            text_ops::toggle_comment(&document.text, &self.clamped_selection(&document.text));
        self.apply(document, edited);
    }

    pub fn open_search(&mut self, document: &Document) {
        let range = self.clamped_selection(&document.text).range();
        let selected = &document.text[range];
        if !selected.is_empty() && !selected.contains('\n') {
            self.search.query = selected.to_owned();
        }
        self.search.open = true;
        self.search.focus_query = true;
        self.search.computed_for = 0;
    }

    pub fn search_is_open(&self) -> bool {
        self.search.open
    }

    pub fn find_next(&mut self, document: &Document, forward: bool) {
        self.update_matches(&document.text);
        let count = self.search.matches.len();
        if count == 0 {
            return;
        }
        self.search.current = if forward {
            (self.search.current + 1) % count
        } else {
            (self.search.current + count - 1) % count
        };
        self.select_current_match(&document.text);
    }

    /// Asks for completions at the cursor on the next frame.
    pub fn request_completion(&mut self) {
        self.completion_wanted = true;
    }

    /// Completion results from the worker. Returns `false` when there is
    /// nothing to show.
    pub fn show_completions(
        &mut self,
        items: Vec<CompletionItem>,
        signatures: Vec<String>,
    ) -> bool {
        self.calltip = None;
        self.completion.open = false;
        if !items.is_empty() {
            self.completion = Completion {
                items,
                selected: 0,
                anchor: self.completion_anchor,
                open: true,
                scroll_to_selected: true,
            };
            true
        } else if !signatures.is_empty() {
            self.calltip = Some(signatures.join("\n"));
            true
        } else {
            false
        }
    }

    fn close_popups(&mut self) {
        self.completion.open = false;
        self.calltip = None;
    }

    fn clamped_selection(&self, text: &str) -> Selection {
        let clamp = |i: usize| floor_char_boundary(text, i.min(text.len()));
        Selection {
            anchor: clamp(self.selection.anchor),
            cursor: clamp(self.selection.cursor),
        }
    }

    fn apply(&mut self, document: &mut Document, edited: text_ops::Edited) {
        if edited.text != document.text {
            document.text = edited.text;
        }
        self.selection = edited.selection.clone();
        self.pending_selection = Some(edited.selection);
        self.pending_focus = true;
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Document,
        options: &EditorOptions<'_>,
    ) -> EditorOutput {
        let mut output = EditorOutput::default();
        if self.search.open {
            self.search_bar(ui, document);
        }

        let font = FontId::monospace(options.font_size);
        let (row_height, char_width) =
            ui.fonts_mut(|f| (f.row_height(&font), f.glyph_width(&font, 'M')));

        // Keys the editor handles itself must be taken before TextEdit sees them.
        self.selection = self.read_selection(ui.ctx(), &document.text);
        if ui.memory(|m| m.has_focus(self.text_id)) {
            output.changed |= self.handle_keys(ui, document, options);
        }
        if let Some(selection) = self.pending_selection.take() {
            self.write_selection(ui.ctx(), &document.text, &selection);
        }
        if std::mem::take(&mut self.pending_focus) {
            ui.memory_mut(|m| m.request_focus(self.text_id));
        }

        let line_count = document.text.matches('\n').count() + 1;
        let digits = line_count.to_string().len().max(2);
        let gutter_width = digits as f32 * char_width + 28.0;

        let dark = ui.visuals().dark_mode;
        let colors = options.colors;
        let area = ui.available_rect_before_wrap();
        let gutter_rect = Rect::from_min_size(area.min, Vec2::new(gutter_width, area.height()));
        let text_rect = Rect::from_min_max(Pos2::new(gutter_rect.right(), area.top()), area.max);
        ui.allocate_rect(area, Sense::hover());

        // The text scrolls both ways; the gutter only follows it vertically,
        // so the line numbers stay in view.
        let mut text_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        let geometry = egui::ScrollArea::both()
            .id_salt("code_editor_scroll")
            .auto_shrink([false, false])
            .show(&mut text_ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                // Placeholder for backgrounds that must sit under the text.
                let background = ui.painter().add(Shape::Noop);

                let wrap = options.line_wrap;
                let previous_width = self.layout.width;
                let cache = &mut self.layout;
                let mut layouter = |ui: &egui::Ui, buffer: &dyn TextBuffer, wrap_width: f32| {
                    let text = buffer.as_str();
                    let wrap_width = if wrap { wrap_width } else { f32::INFINITY };
                    let key = layout_key(text, wrap_width, font.size, dark);
                    if cache.key != key || cache.job.is_none() {
                        cache.key = key;
                        cache.job = Some(highlight::layout_job(
                            text,
                            font.clone(),
                            &colors.syntax,
                            wrap_width,
                        ));
                    }
                    let job = cache.job.clone().unwrap_or_default();
                    let galley = ui.fonts_mut(|f| f.layout_job(job));
                    cache.width = galley.size().x;
                    galley
                };

                let available = ui.available_size();
                // Without wrapping the text edit must be as wide as its
                // longest line, so the scroll area can scroll sideways
                // (last frame's layout is a good enough measure).
                let text_width = if wrap {
                    available.x
                } else {
                    (previous_width + 24.0).max(available.x)
                };
                let edit = TextEdit::multiline(&mut document.text)
                    .id(self.text_id)
                    .font(font.clone())
                    .lock_focus(true)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::symmetric(6, 2))
                    .desired_width(if wrap { available.x } else { f32::INFINITY })
                    .min_size(Vec2::new(text_width, available.y.max(row_height)))
                    .layouter(&mut layouter)
                    .show(ui);
                let text_changed = edit.response.response.changed();
                output.changed |= text_changed;
                if let Some(range) = edit.cursor_range {
                    self.selection = Selection {
                        anchor: text_ops::char_to_byte(&document.text, range.secondary.index.0),
                        cursor: text_ops::char_to_byte(&document.text, range.primary.index.0),
                    };
                }
                if text_changed {
                    self.close_popups_if_left(&document.text);
                }

                // Typing "." asks for member completions.
                let typed_dot = text_changed
                    && ui.input(|i| {
                        i.events
                            .iter()
                            .any(|e| matches!(e, egui::Event::Text(t) if t == "."))
                    });
                let wanted = std::mem::take(&mut self.completion_wanted);
                if typed_dot || wanted {
                    let (line, column) =
                        text_ops::line_column(&document.text, self.selection.cursor);
                    self.completion_anchor = self.selection.cursor;
                    output.completion_request = Some(CompletionRequest {
                        line: line + 1,
                        column,
                    });
                }

                let geometry = Geometry::new(&edit.galley, edit.galley_pos);
                let text_left = edit.response.response.rect.left();
                let shapes = self.backgrounds(
                    &geometry,
                    &document.text,
                    options,
                    text_left,
                    ui.clip_rect(),
                    char_width,
                );
                ui.painter().set(background, Shape::Vec(shapes));

                if let Some(line) = self.pending_scroll_line.take()
                    && let Some(rect) = geometry.line_rect(line.saturating_sub(1))
                {
                    // Aim at the start of the line: vertical scrolling only.
                    let target = Rect::from_min_size(
                        Pos2::new(text_left, rect.top()),
                        Vec2::new(1.0, rect.height()),
                    );
                    ui.scroll_to_rect(target, Some(Align::Center));
                }

                let caret = geometry.caret_rect(&document.text, self.selection.cursor);
                self.popups(ui, document, caret, row_height);
                geometry
            })
            .inner;

        self.paint_gutter(ui, &geometry, gutter_rect, colors);
        let gutter = ui.interact(gutter_rect, self.text_id.with("gutter"), Sense::click());
        if gutter.clicked()
            && let Some(line) = gutter
                .interact_pointer_pos()
                .and_then(|p| geometry.line_at(p.y))
        {
            self.toggle_breakpoint(line + 1);
        }
        gutter.on_hover_cursor(egui::CursorIcon::PointingHand);
        output
    }

    fn read_selection(&self, ctx: &egui::Context, text: &str) -> Selection {
        TextEdit::load_state(ctx, self.text_id)
            .and_then(|state| state.cursor.char_range())
            .map_or(self.selection.clone(), |range| Selection {
                anchor: text_ops::char_to_byte(text, range.secondary.index.0),
                cursor: text_ops::char_to_byte(text, range.primary.index.0),
            })
    }

    fn write_selection(&self, ctx: &egui::Context, text: &str, selection: &Selection) {
        let mut state = TextEdit::load_state(ctx, self.text_id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange {
            primary: CCursor::new(text_ops::byte_to_char(text, selection.cursor)),
            secondary: CCursor::new(text_ops::byte_to_char(text, selection.anchor)),
            h_pos: None,
        }));
        TextEdit::store_state(ctx, self.text_id, state);
    }

    /// Handles Tab, Shift+Tab, Enter and the completion popup's keys.
    fn handle_keys(
        &mut self,
        ui: &egui::Ui,
        document: &mut Document,
        options: &EditorOptions<'_>,
    ) -> bool {
        let consume = |modifiers, key| ui.input_mut(|i| i.consume_key(modifiers, key));
        let selection = self.clamped_selection(&document.text);

        if self.completion.open {
            let visible = self.visible_completions(&document.text);
            if consume(Modifiers::NONE, Key::Escape) {
                self.completion.open = false;
            } else if consume(Modifiers::NONE, Key::ArrowDown) {
                self.completion.selected =
                    (self.completion.selected + 1).min(visible.len().saturating_sub(1));
                self.completion.scroll_to_selected = true;
            } else if consume(Modifiers::NONE, Key::ArrowUp) {
                self.completion.selected = self.completion.selected.saturating_sub(1);
                self.completion.scroll_to_selected = true;
            } else if !visible.is_empty()
                && (consume(Modifiers::NONE, Key::Enter) || consume(Modifiers::NONE, Key::Tab))
            {
                let index = visible[self.completion.selected.min(visible.len() - 1)];
                self.accept_completion(document, index);
                return true;
            }
        }
        if self.calltip.is_some() && consume(Modifiers::NONE, Key::Escape) {
            self.calltip = None;
        }

        let edited = if consume(Modifiers::SHIFT, Key::Tab) {
            Some(text_ops::unindent(&document.text, &selection))
        } else if consume(Modifiers::NONE, Key::Tab) {
            Some(text_ops::indent(&document.text, &selection))
        } else if options.auto_indent && consume(Modifiers::NONE, Key::Enter) {
            let range = selection.range();
            let insert = text_ops::newline_with_indent(&document.text, range.start);
            let mut text = document.text.clone();
            text.replace_range(range.clone(), &insert);
            Some(text_ops::Edited {
                text,
                selection: Selection::caret(range.start + insert.len()),
            })
        } else {
            None
        };
        match edited {
            Some(edited) => {
                self.apply(document, edited);
                true
            }
            None => false,
        }
    }

    /// Indices of the completion items matching what was typed since the
    /// completion was requested.
    fn visible_completions(&self, text: &str) -> Vec<usize> {
        let typed = typed_since(text, self.completion.anchor, self.selection.cursor);
        self.completion
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.insert.starts_with(typed))
            .map(|(i, _)| i)
            .collect()
    }

    fn accept_completion(&mut self, document: &mut Document, index: usize) {
        let Some(item) = self.completion.items.get(index) else {
            return;
        };
        let cursor = self.selection.cursor.min(document.text.len());
        let typed = typed_since(&document.text, self.completion.anchor, cursor).len();
        let insert = item.insert.get(typed..).unwrap_or_default().to_owned();
        let mut text = document.text.clone();
        text.insert_str(cursor, &insert);
        self.completion.open = false;
        self.apply(
            document,
            text_ops::Edited {
                text,
                selection: Selection::caret(cursor + insert.len()),
            },
        );
    }

    /// Typing outside the identifier being completed closes the popups.
    fn close_popups_if_left(&mut self, text: &str) {
        if !self.completion.open {
            return;
        }
        let (anchor, cursor) = (self.completion.anchor, self.selection.cursor);
        let typed = (cursor >= anchor && cursor <= text.len())
            .then(|| &text[anchor.min(text.len())..cursor]);
        if typed.is_none_or(|t| !t.chars().all(|c| c == '_' || c.is_alphanumeric())) {
            self.completion.open = false;
        }
    }

    fn search_bar(&mut self, ui: &mut egui::Ui, document: &Document) {
        self.update_matches(&document.text);
        let mut navigate: Option<bool> = None;
        let mut close = false;
        ui.horizontal(|ui| {
            ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
            let response = ui.add(
                TextEdit::singleline(&mut self.search.query)
                    .hint_text("Search")
                    .desired_width(220.0),
            );
            if std::mem::take(&mut self.search.focus_query) {
                response.request_focus();
            }
            if response.changed() {
                self.search.computed_for = 0;
                self.search.current = 0;
            }
            if response.has_focus() || response.lost_focus() {
                let (enter, shift, escape) = ui.input(|i| {
                    (
                        i.key_pressed(Key::Enter),
                        i.modifiers.shift,
                        i.key_pressed(Key::Escape),
                    )
                });
                if enter {
                    navigate = Some(!shift);
                    response.request_focus();
                }
                close |= escape;
            }
            if ui
                .add_enabled(
                    !self.search.matches.is_empty(),
                    egui::Button::new(egui_phosphor::regular::ARROW_UP),
                )
                .on_hover_text("Previous match (Shift+F3)")
                .clicked()
            {
                navigate = Some(false);
            }
            if ui
                .add_enabled(
                    !self.search.matches.is_empty(),
                    egui::Button::new(egui_phosphor::regular::ARROW_DOWN),
                )
                .on_hover_text("Next match (F3)")
                .clicked()
            {
                navigate = Some(true);
            }
            if ui
                .selectable_label(self.search.match_case, "Aa")
                .on_hover_text("Match case")
                .clicked()
            {
                self.search.match_case = !self.search.match_case;
                self.search.computed_for = 0;
            }
            let count = self.search.matches.len();
            ui.weak(match count {
                0 if self.search.query.is_empty() => String::new(),
                0 => "No matches".to_owned(),
                _ => format!("{} of {count}", self.search.current.min(count - 1) + 1),
            });
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button(egui_phosphor::regular::X)
                    .on_hover_text("Close (Esc)")
                    .clicked()
                {
                    close = true;
                }
            });
        });
        ui.separator();

        if let Some(forward) = navigate {
            self.find_next(document, forward);
        }
        if close {
            self.search.open = false;
            self.pending_focus = true;
        }
    }

    fn update_matches(&mut self, text: &str) {
        let mut hasher = DefaultHasher::new();
        (text, &self.search.query, self.search.match_case).hash(&mut hasher);
        let key = hasher.finish() | 1;
        if self.search.computed_for == key {
            return;
        }
        self.search.computed_for = key;
        self.search.matches = find_all(text, &self.search.query, self.search.match_case);
        if self.search.current >= self.search.matches.len() {
            self.search.current = 0;
        }
    }

    fn select_current_match(&mut self, text: &str) {
        if let Some(range) = self.search.matches.get(self.search.current).cloned() {
            self.pending_selection = Some(Selection {
                anchor: range.start,
                cursor: range.end,
            });
            self.pending_scroll_line = Some(text_ops::line_of(text, range.start) + 1);
        }
    }

    fn backgrounds(
        &self,
        geometry: &Geometry,
        text: &str,
        options: &EditorOptions<'_>,
        text_left: f32,
        clip: Rect,
        char_width: f32,
    ) -> Vec<Shape> {
        let colors = options.colors;
        let mut shapes = Vec::new();
        let full_width = |rect: Rect| {
            Rect::from_x_y_ranges(text_left..=clip.right().max(rect.right()), rect.y_range())
        };

        let mut line_background = |line: Option<usize>, color: Color32| {
            if let Some(rect) = line.and_then(|l| geometry.line_rect(l.saturating_sub(1)))
                && rect.intersects(clip)
            {
                shapes.push(Shape::rect_filled(full_width(rect), 0.0, color));
            }
        };
        if options.highlight_current_line && self.selection.range().is_empty() {
            line_background(
                Some(text_ops::line_of(text, self.selection.cursor) + 1),
                colors.current_line,
            );
        }
        line_background(self.error_line, colors.error_line);
        line_background(self.debug_line, colors.debug_line);

        if self.search.open {
            for (i, range) in self
                .search
                .matches
                .iter()
                .enumerate()
                .take(MAX_HIGHLIGHTED_MATCHES)
            {
                let start = geometry.caret_rect(text, range.start);
                let end = geometry.caret_rect(text, range.end);
                if !start.intersects(clip) {
                    continue;
                }
                let rect = if (start.top() - end.top()).abs() < 1.0 {
                    Rect::from_min_max(start.left_top(), Pos2::new(end.left(), start.bottom()))
                } else {
                    Rect::from_min_max(
                        start.left_top(),
                        Pos2::new(start.left() + char_width, start.bottom()),
                    )
                };
                let color = if i == self.search.current {
                    colors.search_current
                } else {
                    colors.search_match
                };
                shapes.push(Shape::rect_filled(
                    rect.expand2(Vec2::new(1.0, 0.0)),
                    2.0,
                    color,
                ));
            }
        }

        if options.ruler > 0 {
            let x = geometry.origin.x + options.ruler as f32 * char_width;
            if (clip.left()..=clip.right()).contains(&x) {
                shapes.push(Shape::line_segment(
                    [Pos2::new(x, clip.top()), Pos2::new(x, clip.bottom())],
                    Stroke::new(1.0, colors.ruler),
                ));
            }
        }
        shapes
    }

    fn paint_gutter(
        &self,
        ui: &egui::Ui,
        geometry: &Geometry,
        gutter: Rect,
        colors: &EditorColors,
    ) {
        let painter = ui.painter_at(gutter);
        let clip = gutter;
        let font = FontId::monospace(geometry.row_height.max(8.0) * 0.8);
        let cursor_line = geometry.line_of_byte(self.selection.cursor);
        for (line, rect) in geometry.lines() {
            if rect.bottom() < clip.top() || rect.top() > clip.bottom() {
                continue;
            }
            let number = line + 1;
            let y_center = rect.top() + geometry.row_height / 2.0;
            let color = if self.error_line == Some(number) {
                colors.error_text
            } else if Some(line) == cursor_line {
                colors.gutter_text_current
            } else {
                colors.gutter_text
            };
            painter.text(
                Pos2::new(gutter.right() - 10.0, rect.top()),
                Align2::RIGHT_TOP,
                number.to_string(),
                font.clone(),
                color,
            );
            let marker = Pos2::new(gutter.left() + 10.0, y_center);
            if self.breakpoints.contains(&number) {
                painter.circle(
                    marker,
                    4.5,
                    colors.breakpoint,
                    Stroke::new(1.0, colors.breakpoint.gamma_multiply(0.6)),
                );
            }
            if self.debug_line == Some(number) {
                let arrow = vec![
                    marker + Vec2::new(-4.0, -5.0),
                    marker + Vec2::new(5.0, 0.0),
                    marker + Vec2::new(-4.0, 5.0),
                ];
                painter.add(Shape::convex_polygon(
                    arrow,
                    colors.debug_marker,
                    Stroke::NONE,
                ));
            }
        }
    }

    fn popups(&mut self, ui: &egui::Ui, document: &mut Document, caret: Rect, row_height: f32) {
        let anchor = caret.left_bottom() + Vec2::new(0.0, 2.0);
        if self.completion.open {
            let visible = self.visible_completions(&document.text);
            if visible.is_empty() {
                self.completion.open = false;
            } else {
                self.completion.selected = self.completion.selected.min(visible.len() - 1);
                let mut clicked = None;
                let scroll = std::mem::take(&mut self.completion.scroll_to_selected);
                egui::Area::new(self.text_id.with("completion"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(anchor)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_min_width(220.0);
                            egui::ScrollArea::vertical()
                                .max_height(row_height * COMPLETION_ROWS as f32 * 1.6)
                                .show(ui, |ui| {
                                    for (position, &index) in visible.iter().enumerate() {
                                        let item = &self.completion.items[index];
                                        let selected = position == self.completion.selected;
                                        let label =
                                            format!("{}  {}", kind_icon(&item.kind), item.name);
                                        let response = ui.selectable_label(selected, label);
                                        if selected && scroll {
                                            response.scroll_to_me(None);
                                        }
                                        if response.clicked() {
                                            clicked = Some(index);
                                        }
                                    }
                                });
                        });
                    });
                if let Some(index) = clicked {
                    self.accept_completion(document, index);
                }
            }
        }
        if let Some(calltip) = &self.calltip {
            egui::Area::new(self.text_id.with("calltip"))
                .order(egui::Order::Foreground)
                .fixed_pos(anchor)
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.monospace(calltip);
                    });
                });
        }
    }
}

/// Text typed between `anchor` and `cursor` (empty if the cursor moved back).
fn typed_since(text: &str, anchor: usize, cursor: usize) -> &str {
    if cursor >= anchor
        && cursor <= text.len()
        && text.is_char_boundary(anchor)
        && text.is_char_boundary(cursor)
    {
        &text[anchor..cursor]
    } else {
        ""
    }
}

fn kind_icon(kind: &str) -> &'static str {
    use egui_phosphor::regular as icon;
    match kind {
        "function" => icon::FUNCTION,
        "class" => icon::CUBE,
        "module" => icon::PACKAGE,
        "keyword" => icon::TEXT_AA,
        "param" => icon::AT,
        "property" | "statement" | "instance" => icon::DOT_OUTLINE,
        _ => icon::DOT,
    }
}

fn layout_key(text: &str, wrap_width: f32, font_size: f32, dark: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    (text, wrap_width.to_bits(), font_size.to_bits(), dark).hash(&mut hasher);
    hasher.finish()
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Byte ranges of every occurrence of `query`.
fn find_all(text: &str, query: &str, match_case: bool) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    if match_case {
        return text
            .match_indices(query)
            .map(|(i, m)| i..i + m.len())
            .collect();
    }
    // Lowercasing can change byte lengths for some scripts, so compare
    // character by character.
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let haystack: Vec<(usize, char)> = text
        .char_indices()
        .flat_map(|(i, c)| c.to_lowercase().map(move |l| (i, l)))
        .collect();
    let mut matches = Vec::new();
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if haystack[i..i + needle.len()]
            .iter()
            .map(|(_, c)| *c)
            .eq(needle.iter().copied())
        {
            let start = haystack[i].0;
            let last = haystack[i + needle.len() - 1].0;
            let end = last + text[last..].chars().next().map_or(0, char::len_utf8);
            matches.push(start..end);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    matches
}

/// Where lines and characters ended up on screen.
struct Geometry {
    galley: Arc<Galley>,
    origin: Pos2,
    row_height: f32,
    /// First row index of each logical line.
    line_rows: Vec<usize>,
}

impl Geometry {
    fn new(galley: &Arc<Galley>, origin: Pos2) -> Self {
        let mut line_rows = vec![0];
        for (index, row) in galley.rows.iter().enumerate() {
            if row.ends_with_newline {
                line_rows.push(index + 1);
            }
        }
        let row_height = galley.rows.first().map_or(14.0, |row| row.rect().height());
        Self {
            galley: galley.clone(),
            origin,
            row_height,
            line_rows,
        }
    }

    /// Screen rectangle of a (0-based) logical line, across its wrapped rows.
    fn line_rect(&self, line: usize) -> Option<Rect> {
        let first = *self.line_rows.get(line)?;
        let last = self
            .line_rows
            .get(line + 1)
            .map_or(self.galley.rows.len(), |&next| next)
            .max(first + 1);
        let top = self
            .galley
            .rows
            .get(first)
            .map_or_else(|| self.galley.rect.bottom(), |row| row.rect().top());
        let bottom = self
            .galley
            .rows
            .get(last - 1)
            .map_or(top + self.row_height, |row| row.rect().bottom());
        Some(Rect::from_min_max(
            Pos2::new(self.origin.x, self.origin.y + top),
            Pos2::new(
                self.origin.x + self.galley.rect.width(),
                self.origin.y + bottom,
            ),
        ))
    }

    fn lines(&self) -> impl Iterator<Item = (usize, Rect)> + '_ {
        (0..self.line_rows.len()).filter_map(|line| self.line_rect(line).map(|rect| (line, rect)))
    }

    fn line_at(&self, y: f32) -> Option<usize> {
        self.lines()
            .find(|(_, rect)| rect.y_range().contains(y))
            .map(|(line, _)| line)
    }

    fn line_of_byte(&self, byte: usize) -> Option<usize> {
        let text = self.galley.text();
        Some(text_ops::line_of(text, byte.min(text.len())))
    }

    /// Screen rectangle of the caret at `byte`.
    fn caret_rect(&self, text: &str, byte: usize) -> Rect {
        let index = text_ops::byte_to_char(text, byte.min(text.len()));
        self.galley
            .pos_from_cursor(CCursor::new(index))
            .translate(self.origin.to_vec2())
    }
}

#[cfg(test)]
#[path = "../../tests/rust/editor/test_editor.rs"]
mod tests;
