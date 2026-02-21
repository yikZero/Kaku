use crate::commands::{CommandDef, ExpandedCommand};
use crate::overlay::selector::{matcher_pattern, matcher_score};
use crate::termwindow::box_model::*;
use crate::termwindow::modal::Modal;
use crate::termwindow::{DimensionContext, TermWindow};
use crate::utilsprites::RenderMetrics;
use config::keyassignment::KeyAssignment;
use config::Dimension;
use frecency::Frecency;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::{Ref, RefCell};
use std::cmp::Ordering;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wezterm_term::{KeyCode, KeyModifiers, MouseEvent};
use window::color::LinearRgba;
use window::WindowOps;

// Kaku theme colors for Command Palette
const KAKU_BG: LinearRgba = LinearRgba::with_components(0.082, 0.078, 0.106, 0.95); // #15141b with opacity
const KAKU_FG: LinearRgba = LinearRgba::with_components(0.929, 0.925, 0.933, 1.0); // #edecee
const KAKU_ACCENT: LinearRgba = LinearRgba::with_components(0.635, 0.467, 1.0, 1.0); // #a277ff
const KAKU_SELECTION_BG: LinearRgba = LinearRgba::with_components(0.161, 0.149, 0.235, 1.0); // #29263c
const KAKU_DIM_FG: LinearRgba = LinearRgba::with_components(0.420, 0.420, 0.420, 1.0); // #6b6b6b
const KAKU_BORDER: LinearRgba = LinearRgba::with_components(0.2, 0.18, 0.28, 0.6);

struct MatchResults {
    selection: String,
    matches: Vec<usize>,
}

// Cache state to track when we need to rebuild the UI
struct CacheState {
    selection: String,
    selected_row: usize,
    top_row: usize,
    max_rows: usize,
    pixel_width: usize,
    pixel_height: usize,
}

struct PaletteBounds {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

pub struct CommandPalette {
    element: RefCell<Option<Vec<ComputedElement>>>,
    cache_state: RefCell<Option<CacheState>>,
    selection: RefCell<String>,
    matches: RefCell<Option<MatchResults>>,
    selected_row: RefCell<usize>,
    top_row: RefCell<usize>,
    max_rows_on_screen: RefCell<usize>,
    commands: Vec<ExpandedCommand>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Recent {
    brief: String,
    frecency: Frecency,
}

fn recent_file_name() -> PathBuf {
    config::DATA_DIR.join("recent-commands.json")
}

fn load_recents() -> anyhow::Result<Vec<Recent>> {
    let file_name = recent_file_name();
    let f = std::fs::File::open(&file_name)?;
    let mut recents: Vec<Recent> = serde_json::from_reader(f)?;
    recents.sort_by(|a, b| {
        b.frecency
            .score()
            .partial_cmp(&a.frecency.score())
            .unwrap_or(Ordering::Equal)
    });
    Ok(recents)
}

fn save_recent(command: &ExpandedCommand) -> anyhow::Result<()> {
    let mut recents = load_recents().unwrap_or_else(|_| vec![]);
    if let Some(recent_idx) = recents.iter().position(|r| r.brief == command.brief) {
        let recent = recents.get_mut(recent_idx).unwrap();
        recent.frecency.register_access();
    } else {
        let mut frecency = Frecency::new();
        frecency.register_access();
        recents.push(Recent {
            brief: command.brief.to_string(),
            frecency,
        });
    }

    let json = serde_json::to_string(&recents)?;
    let file_name = recent_file_name();
    std::fs::write(&file_name, json)?;
    Ok(())
}

fn build_commands() -> Vec<ExpandedCommand> {
    // Fast path: return static list without sorting or frecency
    CommandDef::actions_for_palette_only(&config::configuration())
}

#[derive(Debug)]
struct MatchResult {
    row_idx: usize,
    score: u32,
}

impl MatchResult {
    fn new(row_idx: usize, score: u32, selection: &str, commands: &[ExpandedCommand]) -> Self {
        Self {
            row_idx,
            score: if commands[row_idx].brief == selection {
                // Pump up the score for an exact match, otherwise
                // the order may be undesirable if there are a lot
                // of candidates with the same score
                u32::max_value()
            } else {
                score
            },
        }
    }
}

fn compute_matches(selection: &str, commands: &[ExpandedCommand]) -> Vec<usize> {
    if selection.is_empty() {
        commands.iter().enumerate().map(|(idx, _)| idx).collect()
    } else {
        let pattern = matcher_pattern(selection);

        let start = std::time::Instant::now();
        let mut scores: Vec<MatchResult> = commands
            .par_iter()
            .enumerate()
            .filter_map(|(row_idx, entry)| {
                let group = entry.menubar.join(" ");
                let text = format!("{group}: {}. {} {:?}", entry.brief, entry.doc, entry.action);
                matcher_score(&pattern, &text)
                    .map(|score| MatchResult::new(row_idx, score, selection, commands))
            })
            .collect();
        scores.sort_by(|a, b| a.score.cmp(&b.score).reverse());
        log::trace!("matching took {:?}", start.elapsed());

        scores.iter().map(|result| result.row_idx).collect()
    }
}

impl CommandPalette {
    fn palette_bounds(term_window: &TermWindow, metrics: &RenderMetrics) -> PaletteBounds {
        let top_bar_height = if term_window.show_tab_bar && !term_window.config.tab_bar_at_bottom {
            term_window.tab_bar_pixel_height().unwrap_or(0.0)
        } else {
            0.0
        };
        let (padding_left, padding_top) = term_window.padding_left_top();
        let border = term_window.get_os_border();

        let content_x = padding_left + border.left.get() as f32;
        let content_y = top_bar_height + padding_top + border.top.get() as f32;

        let content_width = term_window.terminal_size.pixel_width as f32;
        let content_height = term_window.terminal_size.pixel_height as f32;

        let cell_width = metrics.cell_size.width as f32;
        let cell_height = metrics.cell_size.height as f32;

        let max_width = (content_width - cell_width * 2.0).max(cell_width * 24.0);
        let max_height = (content_height - cell_height * 2.0).max(cell_height * 12.0);

        let palette_width_target = (content_width * 0.72).clamp(760.0, 1080.0).min(max_width);
        let palette_cols = (palette_width_target / cell_width).floor().max(24.0);
        let palette_width = (palette_cols * cell_width).min(max_width);
        let palette_height = (content_height * 0.72).min(max_height);

        let x = content_x + ((content_width - palette_width) / 2.0).max(0.0);
        let y = content_y + ((content_height - palette_height) / 2.0).max(0.0);

        PaletteBounds {
            x,
            y,
            width: palette_width,
            height: palette_height,
        }
    }

    pub fn new(_term_window: &mut TermWindow) -> Self {
        let commands = build_commands();

        Self {
            element: RefCell::new(None),
            cache_state: RefCell::new(None),
            selection: RefCell::new(String::new()),
            commands,
            matches: RefCell::new(None),
            selected_row: RefCell::new(0),
            top_row: RefCell::new(0),
            max_rows_on_screen: RefCell::new(0),
        }
    }

    fn compute(
        term_window: &mut TermWindow,
        selection: &str,
        commands: &[ExpandedCommand],
        matches: &MatchResults,
        max_rows_on_screen: usize,
        selected_row: usize,
        top_row: usize,
    ) -> anyhow::Result<Vec<ComputedElement>> {
        let font = term_window
            .fonts
            .command_palette_font()
            .expect("to resolve command palette font");
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let dimensions = term_window.dimensions;
        let bounds = Self::palette_bounds(term_window, &metrics);
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));
        let blink_period_ms = 1000u128;
        let on_phase_ms = 550u128;
        let phase = epoch.as_millis() % blink_period_ms;
        let cursor_visible = phase < on_phase_ms;
        let ms_to_next_toggle = if cursor_visible {
            on_phase_ms.saturating_sub(phase)
        } else {
            blink_period_ms.saturating_sub(phase)
        };
        term_window.update_next_frame_time(Some(
            std::time::Instant::now()
                + Duration::from_millis(ms_to_next_toggle.max(1).min(u128::from(u64::MAX)) as u64),
        ));

        // Search input area
        let mut elements = vec![];

        // Search box with explicit blinking caret so focus is obvious.
        let mut search_row = vec![Element::new(&font, ElementContent::Text("⌘ ".to_string()))
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: LinearRgba::TRANSPARENT.into(),
                text: KAKU_ACCENT.into(),
            })];
        let caret = if cursor_visible { "▏" } else { " " };
        if selection.is_empty() {
            search_row.push(
                Element::new(&font, ElementContent::Text(caret.to_string())).colors(
                    ElementColors {
                        border: BorderColor::default(),
                        bg: LinearRgba::TRANSPARENT.into(),
                        text: KAKU_ACCENT.into(),
                    },
                ),
            );
            search_row.push(
                Element::new(
                    &font,
                    ElementContent::Text(" Type to search...".to_string()),
                )
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: LinearRgba::TRANSPARENT.into(),
                    text: KAKU_DIM_FG.into(),
                }),
            );
        } else {
            search_row.push(
                Element::new(&font, ElementContent::Text(selection.to_string())).colors(
                    ElementColors {
                        border: BorderColor::default(),
                        bg: LinearRgba::TRANSPARENT.into(),
                        text: KAKU_FG.into(),
                    },
                ),
            );
            search_row.push(
                Element::new(&font, ElementContent::Text(caret.to_string())).colors(
                    ElementColors {
                        border: BorderColor::default(),
                        bg: LinearRgba::TRANSPARENT.into(),
                        text: KAKU_ACCENT.into(),
                    },
                ),
            );
        }

        elements.push(
            Element::new(&font, ElementContent::Children(search_row))
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: LinearRgba::TRANSPARENT.into(),
                    text: KAKU_FG.into(),
                })
                .padding(BoxDimension {
                    left: Dimension::Cells(1.0),
                    right: Dimension::Cells(1.0),
                    top: Dimension::Cells(0.6),
                    bottom: Dimension::Cells(0.6),
                })
                .min_width(Some(Dimension::Percent(1.0)))
                .display(DisplayType::Block),
        );

        // Separator line
        elements.push(
            Element::new(&font, ElementContent::Text("".to_string()))
                .colors(ElementColors {
                    border: BorderColor::new(KAKU_BORDER.into()),
                    bg: LinearRgba::TRANSPARENT.into(),
                    text: KAKU_FG.into(),
                })
                .display(DisplayType::Block)
                .min_height(Some(Dimension::Pixels(1.0)))
                .margin(BoxDimension {
                    left: Dimension::Cells(0.6),
                    right: Dimension::Cells(0.6),
                    top: Dimension::Cells(0.),
                    bottom: Dimension::Cells(0.),
                }),
        );

        // Results list - only render visible rows for performance
        let visible_commands: Vec<_> = matches
            .matches
            .iter()
            .map(|&idx| &commands[idx])
            .enumerate()
            .skip(top_row)
            .take(max_rows_on_screen)
            .collect();

        for (display_idx, command) in visible_commands {
            let is_selected = display_idx == selected_row;

            let bg: InheritableColor = if is_selected {
                KAKU_SELECTION_BG.into()
            } else {
                LinearRgba::TRANSPARENT.into()
            };

            let label = command.brief.to_string();

            // Build row with better spacing
            let mut row = vec![Element::new(
                &font,
                ElementContent::Text(format!("  {}", label)),
            )];

            // Keyboard shortcut with better spacing
            if let Some((mods, keycode)) = command.keys.first() {
                let ui_rendering = term_window.config.ui_key_cap_rendering;
                let separator = if ui_rendering == ::window::UIKeyCapRendering::AppleSymbols {
                    " "
                } else {
                    " + "
                };
                let mod_string = mods.to_string_with_separator(::window::ModifierToStringArgs {
                    separator,
                    want_none: false,
                    ui_key_cap_rendering: Some(ui_rendering),
                });
                let key_display = crate::inputmap::ui_key(keycode, ui_rendering);
                let key_str = if mod_string.is_empty() {
                    key_display
                } else {
                    format!("{}{}{}", mod_string, separator, key_display)
                };

                // Add visible spacing around shortcuts so key caps don't look crowded.
                row.push(
                    Element::new(&font, ElementContent::Text(format!("  {}  ", key_str)))
                        .float(Float::Right)
                        .colors(ElementColors {
                            border: BorderColor::default(),
                            bg: LinearRgba::TRANSPARENT.into(),
                            text: if is_selected {
                                KAKU_FG.into()
                            } else {
                                KAKU_DIM_FG.into()
                            },
                        }),
                );
            }

            elements.push(
                Element::new(&font, ElementContent::Children(row))
                    .colors(ElementColors {
                        border: BorderColor::default(),
                        bg,
                        text: KAKU_FG.into(),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.6),
                        right: Dimension::Cells(0.6),
                        top: Dimension::Cells(0.4),
                        bottom: Dimension::Cells(0.4),
                    })
                    .min_width(Some(Dimension::Percent(1.0)))
                    .display(DisplayType::Block),
            );
        }

        // Centered floating container with 70% height
        let element = Element::new(&font, ElementContent::Children(elements))
            .colors(ElementColors {
                border: BorderColor::new(KAKU_BORDER.into()),
                bg: KAKU_BG.into(),
                text: KAKU_FG.into(),
            })
            .padding(BoxDimension {
                left: Dimension::Cells(0.),
                right: Dimension::Cells(0.),
                top: Dimension::Cells(0.4),
                bottom: Dimension::Cells(0.4),
            })
            .border(BoxDimension::new(Dimension::Pixels(1.)))
            .min_width(Some(Dimension::Pixels(bounds.width)));

        let computed = term_window.compute_element(
            &LayoutContext {
                height: DimensionContext {
                    dpi: dimensions.dpi as f32,
                    pixel_max: dimensions.pixel_height as f32,
                    pixel_cell: metrics.cell_size.height as f32,
                },
                width: DimensionContext {
                    dpi: dimensions.dpi as f32,
                    pixel_max: dimensions.pixel_width as f32,
                    pixel_cell: metrics.cell_size.width as f32,
                },
                bounds: euclid::rect(bounds.x, bounds.y, bounds.width, bounds.height),
                metrics: &metrics,
                gl_state: term_window.render_state.as_ref().unwrap(),
                zindex: 100,
            },
            &element,
        )?;

        Ok(vec![computed])
    }

    fn updated_input(&self) {
        *self.selected_row.borrow_mut() = 0;
        *self.top_row.borrow_mut() = 0;
    }

    fn move_up(&self) -> bool {
        self.move_by(-1)
    }

    fn move_down(&self) -> bool {
        self.move_by(1)
    }

    fn match_count(&self) -> usize {
        self.matches
            .borrow()
            .as_ref()
            .map(|m| m.matches.len())
            .unwrap_or_else(|| self.commands.len())
    }

    fn visible_rows(&self) -> usize {
        (*self.max_rows_on_screen.borrow()).max(1)
    }

    fn scroll_margin(visible_rows: usize) -> usize {
        if visible_rows <= 3 {
            0
        } else {
            (visible_rows / 6).clamp(1, 3)
        }
    }

    fn align_top_for_row(
        row: usize,
        current_top: usize,
        visible_rows: usize,
        limit: usize,
    ) -> usize {
        let window_rows = visible_rows.max(1);
        let margin = Self::scroll_margin(window_rows);
        let max_top = limit.saturating_sub(window_rows.saturating_sub(1));
        let mut top = current_top.min(max_top);

        let lower = top.saturating_add(margin);
        let upper = top.saturating_add(window_rows.saturating_sub(1 + margin));

        if row < lower {
            top = row.saturating_sub(margin);
        } else if row > upper {
            top = row.saturating_sub(window_rows.saturating_sub(1 + margin));
        }

        top.min(max_top)
    }

    fn set_selected_row(&self, target_row: usize) -> bool {
        let count = self.match_count();
        let limit = count.saturating_sub(1);
        let next_row = target_row.min(limit);

        let current_row = *self.selected_row.borrow();
        let current_top = *self.top_row.borrow();
        let next_top = Self::align_top_for_row(next_row, current_top, self.visible_rows(), limit);

        if next_row == current_row && next_top == current_top {
            return false;
        }

        *self.selected_row.borrow_mut() = next_row;
        *self.top_row.borrow_mut() = next_top;

        // Clear element cache to trigger re-render with new selection.
        self.element.borrow_mut().take();
        true
    }

    fn move_by(&self, delta: isize) -> bool {
        let limit = self
            .matches
            .borrow()
            .as_ref()
            .map(|m| m.matches.len())
            .unwrap_or_else(|| self.commands.len())
            .saturating_sub(1);
        let current_row = *self.selected_row.borrow();
        let next_row = if delta < 0 {
            current_row.saturating_sub(delta.unsigned_abs())
        } else {
            current_row.saturating_add(delta as usize)
        }
        .min(limit);

        self.set_selected_row(next_row)
    }

    fn move_page(&self, pages: isize) -> bool {
        let step = self.visible_rows().saturating_sub(1).max(3) as isize;
        self.move_by(step * pages)
    }

    fn jump_to_start(&self) -> bool {
        self.set_selected_row(0)
    }

    fn jump_to_end(&self) -> bool {
        let limit = self.match_count().saturating_sub(1);
        self.set_selected_row(limit)
    }

    fn smooth_wheel_steps(lines: usize) -> isize {
        let lines = lines.max(1) as f32;
        // Compress large wheel deltas from touchpad inertia so motion feels smoother.
        (lines.sqrt().round() as isize).clamp(1, 3)
    }

    fn activate_selected(&self, term_window: &mut TermWindow) -> bool {
        let selected_idx = *self.selected_row.borrow();
        let alias_idx = match self.matches.borrow().as_ref() {
            None => return true,
            Some(results) => match results.matches.get(selected_idx) {
                Some(i) => *i,
                None => return true,
            },
        };
        let item = &self.commands[alias_idx];
        if let Err(err) = save_recent(item) {
            log::error!("Error while saving recents: {err:#}");
        }
        term_window.cancel_modal();

        if let Some(pane) = term_window.get_active_pane_or_overlay() {
            if let Err(err) = term_window.perform_key_assignment(&pane, &item.action) {
                log::error!("Error while performing {item:?}: {err:#}");
                term_window.show_toast(format!("Command failed: {}", err));
            }
        }
        true
    }

    fn pick_row_from_point(
        &self,
        abs_x: f32,
        abs_y: f32,
        term_window: &mut TermWindow,
    ) -> Option<usize> {
        let clicked_idx = {
            let element = self.element.borrow();
            let root = element.as_ref()?.first()?;
            if abs_x < root.bounds.min_x()
                || abs_x > root.bounds.max_x()
                || abs_y < root.bounds.min_y()
                || abs_y > root.bounds.max_y()
            {
                return None;
            }
            let kids = match &root.content {
                ComputedElementContent::Children(kids) => kids,
                _ => return None,
            };
            kids.iter().position(|kid| {
                abs_x >= kid.bounds.min_x()
                    && abs_x <= kid.bounds.max_x()
                    && abs_y >= kid.bounds.min_y()
                    && abs_y <= kid.bounds.max_y()
            })?
        };

        // 0 = search row, 1 = separator line, 2.. = result rows
        if clicked_idx < 2 {
            return None;
        }
        let top_row = *self.top_row.borrow();
        let visible_idx = clicked_idx - 2;
        let selected = top_row.saturating_add(visible_idx);
        let limit = self
            .matches
            .borrow()
            .as_ref()
            .map(|m| m.matches.len())
            .unwrap_or_else(|| self.commands.len())
            .saturating_sub(1);
        let selected = selected.min(limit);
        *self.selected_row.borrow_mut() = selected;
        self.element.borrow_mut().take();
        if let Some(window) = term_window.window.as_ref() {
            window.invalidate();
        }
        Some(selected)
    }
}

impl Modal for CommandPalette {
    fn perform_assignment(
        &self,
        _assignment: &KeyAssignment,
        _term_window: &mut TermWindow,
    ) -> bool {
        false
    }

    fn mouse_event(&self, event: MouseEvent, term_window: &mut TermWindow) -> anyhow::Result<()> {
        let font = term_window
            .fonts
            .command_palette_font()
            .expect("to resolve command palette font");
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let top_bar_height = if term_window.show_tab_bar && !term_window.config.tab_bar_at_bottom {
            term_window.tab_bar_pixel_height().unwrap_or(0.0)
        } else {
            0.0
        };
        let (padding_left, padding_top) = term_window.padding_left_top();
        let border = term_window.get_os_border();
        let content_x = padding_left + border.left.get() as f32;
        let content_y = top_bar_height + padding_top + border.top.get() as f32;
        let abs_x = content_x
            + event.x as f32 * metrics.cell_size.width as f32
            + event.x_pixel_offset as f32;
        let abs_y = content_y
            + event.y as f32 * metrics.cell_size.height as f32
            + event.y_pixel_offset as f32;

        match event.button {
            wezterm_term::MouseButton::WheelUp(lines) => {
                if self.move_by(-Self::smooth_wheel_steps(lines)) {
                    if let Some(window) = term_window.window.as_ref() {
                        window.invalidate();
                    }
                }
            }
            wezterm_term::MouseButton::WheelDown(lines) => {
                if self.move_by(Self::smooth_wheel_steps(lines)) {
                    if let Some(window) = term_window.window.as_ref() {
                        window.invalidate();
                    }
                }
            }
            wezterm_term::MouseButton::Left => {
                if event.kind == wezterm_term::MouseEventKind::Press
                    && self
                        .pick_row_from_point(abs_x, abs_y, term_window)
                        .is_some()
                {
                    self.activate_selected(term_window);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn key_down(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<bool> {
        let mut needs_invalidate = false;
        match (key, mods) {
            (KeyCode::Escape, KeyModifiers::NONE) | (KeyCode::Char('g'), KeyModifiers::CTRL) => {
                term_window.cancel_modal();
            }
            (KeyCode::PageUp, KeyModifiers::NONE) => {
                needs_invalidate = self.move_page(-1);
            }
            (KeyCode::PageDown, KeyModifiers::NONE) => {
                needs_invalidate = self.move_page(1);
            }
            (KeyCode::Home, KeyModifiers::NONE) | (KeyCode::Char('a'), KeyModifiers::CTRL) => {
                needs_invalidate = self.jump_to_start();
            }
            (KeyCode::End, KeyModifiers::NONE) | (KeyCode::Char('e'), KeyModifiers::CTRL) => {
                needs_invalidate = self.jump_to_end();
            }
            (KeyCode::UpArrow, KeyModifiers::NONE) | (KeyCode::Char('p'), KeyModifiers::CTRL) => {
                needs_invalidate = self.move_up();
            }
            (KeyCode::DownArrow, KeyModifiers::NONE) | (KeyCode::Char('n'), KeyModifiers::CTRL) => {
                needs_invalidate = self.move_down();
            }
            (KeyCode::UpArrow, KeyModifiers::SHIFT) => {
                needs_invalidate = self.move_by(-3);
            }
            (KeyCode::DownArrow, KeyModifiers::SHIFT) => {
                needs_invalidate = self.move_by(3);
            }
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                // Type to add to the selection
                let mut selection = self.selection.borrow_mut();
                selection.push(c);
                self.updated_input();
                needs_invalidate = true;
            }
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                // Backspace to edit the selection
                let mut selection = self.selection.borrow_mut();
                selection.pop();
                self.updated_input();
                needs_invalidate = true;
            }
            (KeyCode::Char('u'), KeyModifiers::CTRL) => {
                // CTRL-u to clear the selection
                let mut selection = self.selection.borrow_mut();
                selection.clear();
                self.updated_input();
                needs_invalidate = true;
            }
            (KeyCode::Enter, KeyModifiers::NONE) => {
                self.activate_selected(term_window);
                return Ok(true);
            }
            // Swallow unhandled keys while palette is open so input never falls through
            // to the terminal pane.
            _ => return Ok(true),
        }
        if needs_invalidate {
            if let Some(window) = term_window.window.as_ref() {
                window.invalidate();
            }
        }
        Ok(true)
    }

    fn computed_element(
        &self,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<Ref<'_, [ComputedElement]>> {
        let selection = self.selection.borrow();
        let selection = selection.as_str();

        let mut results = self.matches.borrow_mut();

        let font = term_window
            .fonts
            .command_palette_font()
            .expect("to resolve char selection font");
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());

        // Calculate max rows based on actual palette height, accounting for row paddings.
        let bounds = Self::palette_bounds(term_window, &metrics);
        let palette_height_px = bounds.height;
        let cell_h = metrics.cell_size.height as f32;
        let row_height = cell_h * 1.8; // cell + top/bottom padding (0.4 + 0.4)
        let search_bar_h = cell_h * 2.2; // cell + top/bottom padding (0.6 + 0.6)
        let overhead = search_bar_h + 1.0 + cell_h * 0.8; // search + separator + container padding
        let available = palette_height_px - overhead;
        let mut max_rows_on_screen = ((available / row_height).floor() as usize).max(5);

        if let Some(size) = term_window.config.command_palette_rows {
            max_rows_on_screen = max_rows_on_screen.min(size);
        }
        *self.max_rows_on_screen.borrow_mut() = max_rows_on_screen;

        let rebuild_matches = results
            .as_ref()
            .map(|m| m.selection != selection)
            .unwrap_or(true);
        if rebuild_matches {
            results.replace(MatchResults {
                selection: selection.to_string(),
                matches: compute_matches(selection, &self.commands),
            });
        }
        let matches = results.as_ref().unwrap();

        // Check if we need to rebuild the UI (selection, scroll position, or size changed)
        let selected_row = *self.selected_row.borrow();
        let top_row = *self.top_row.borrow();
        let dims = term_window.dimensions;

        let needs_rebuild = self.element.borrow().is_none()
            || self.cache_state.borrow().as_ref().map_or(true, |state| {
                state.selection != selection
                    || state.selected_row != selected_row
                    || state.top_row != top_row
                    || state.max_rows != max_rows_on_screen
                    || state.pixel_width != dims.pixel_width
                    || state.pixel_height != dims.pixel_height
            });

        if needs_rebuild {
            let element = Self::compute(
                term_window,
                selection,
                &self.commands,
                matches,
                max_rows_on_screen,
                selected_row,
                top_row,
            )?;
            self.element.borrow_mut().replace(element);
            self.cache_state.borrow_mut().replace(CacheState {
                selection: selection.to_string(),
                selected_row,
                top_row,
                max_rows: max_rows_on_screen,
                pixel_width: dims.pixel_width,
                pixel_height: dims.pixel_height,
            });
        }
        Ok(Ref::map(self.element.borrow(), |v| {
            v.as_ref().unwrap().as_slice()
        }))
    }

    fn reconfigure(&self, _term_window: &mut TermWindow) {
        self.element.borrow_mut().take();
        self.cache_state.borrow_mut().take();
    }
}
