//! Circuit editor UI and project lifecycle.
//!
//! The shell, menus, file dialogs and undo history come from CraftKit. This
//! module owns the palette, the pan/zoom canvas, the netlist and the property
//! inspector; all domain decisions live in the `document`, `logic` and
//! `symbols` modules.

use std::path::PathBuf;

use craftkit_io::DocumentHost;
use craftkit_ui::cli::CliOptions;
use craftkit_ui::eframe::{self, egui};
use craftkit_ui::egui::{Align, Align2, FontId, Layout, Pos2, Rect, RichText, Stroke, Vec2};
use craftkit_ui::{AppIdentity, AppShell, Tone, dialogs, theme, widgets};

use crate::document::{ComponentKind, Document, Pin};
use crate::logic::{self, net_label};
use crate::symbols::{Shape as SymbolShape, symbol_shapes};

const FORMAT: &str = "circuitcraft";
const SCHEMA_VERSION: u32 = 1;
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const GRID: f32 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Wire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    New,
    Open,
    Save,
    SaveAs,
    ExportCsv,
    ExportSvg,
    Undo,
    Redo,
    Delete,
    About,
    Settings,
}

#[derive(Debug, Clone)]
enum PendingAction {
    New,
    Open(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
enum Hit {
    Pin(Pin),
    Component(u64),
    None,
}

#[derive(Debug, Clone)]
struct DragState {
    id: u64,
    start_world: Vec2,
    start_x: f64,
    start_y: f64,
}

/// The circuit editor state.
pub struct CircuitApp {
    shell: AppShell,
    host: DocumentHost<Document>,
    selected: Option<u64>,
    tool: Tool,
    armed_kind: Option<ComponentKind>,
    wire_start: Option<Pin>,
    pan: Vec2,
    zoom: f32,
    needs_fit: bool,
    drag: Option<DragState>,
    prop_gesture_active: bool,
    prop_gain_pending: bool,
    prop_lost_pending: bool,
    pending: Option<PendingAction>,
}

impl CircuitApp {
    /// Build the editor, opening the startup project when requested.
    pub fn new(context: &eframe::CreationContext<'_>, options: CliOptions) -> Self {
        let identity = AppIdentity {
            id: "circuitcraft",
            name: "CircuitCraft",
            version: APP_VERSION,
            tagline: "Schematic capture with netlist and BOM",
            repository: Some("https://github.com/emilio-n8n/circuitcraft"),
        };
        let shell = AppShell::new(identity);
        let document = if options.sample {
            Document::sample()
        } else {
            Document::default()
        };
        let mut app = Self {
            shell,
            host: DocumentHost::new(document, FORMAT, SCHEMA_VERSION, APP_VERSION),
            selected: None,
            tool: Tool::Select,
            armed_kind: None,
            wire_start: None,
            pan: Vec2::new(60.0, 160.0),
            zoom: 1.0,
            needs_fit: true,
            drag: None,
            prop_gesture_active: false,
            prop_gain_pending: false,
            prop_lost_pending: false,
            pending: None,
        };
        app.shell.install_theme(&context.egui_ctx);
        if let Some(path) = options.open {
            app.load_path(path);
        } else if options.sample {
            app.shell.set_status("Sample circuit loaded");
        }
        app
    }

    fn load_path(&mut self, path: PathBuf) {
        self.finish_prop_gesture();
        match DocumentHost::from_file(path.clone(), FORMAT, SCHEMA_VERSION, APP_VERSION) {
            Ok(host) => {
                self.host = host;
                self.reset_interaction();
                self.shell.set_status(format!("Opened {}", path.display()));
            }
            Err(error) => {
                self.shell
                    .notify(Tone::Error, "Could not open project", error.to_string());
            }
        }
    }

    fn new_document(&mut self) {
        self.finish_prop_gesture();
        self.host.reset(Document::default());
        self.reset_interaction();
        self.shell.set_status("New document");
    }

    fn reset_interaction(&mut self) {
        self.selected = None;
        self.tool = Tool::Select;
        self.armed_kind = None;
        self.wire_start = None;
        self.drag = None;
        self.needs_fit = true;
    }

    fn finish_prop_gesture(&mut self) {
        if self.prop_gesture_active {
            self.host.history_mut().end_gesture();
            self.prop_gesture_active = false;
        }
        self.prop_gain_pending = false;
        self.prop_lost_pending = false;
    }

    fn save(&mut self) -> bool {
        self.finish_prop_gesture();
        if self.host.path().is_none() {
            return self.save_as();
        }
        match self.host.save() {
            Ok(path) => {
                self.shell.set_status(format!("Saved {}", path.display()));
                true
            }
            Err(error) => {
                self.shell
                    .notify(Tone::Error, "Could not save project", error.to_string());
                false
            }
        }
    }

    fn save_as(&mut self) -> bool {
        self.finish_prop_gesture();
        let default_name = format!("{}.json", slugify(&self.host.document().title));
        let Some(path) = dialogs::save_project("Save project", &default_name, "json") else {
            return false;
        };
        match self.host.save_as(path) {
            Ok(path) => {
                self.shell.set_status(format!("Saved {}", path.display()));
                true
            }
            Err(error) => {
                self.shell
                    .notify(Tone::Error, "Could not save project", error.to_string());
                false
            }
        }
    }

    fn export_csv(&mut self) {
        let content = logic::bom_csv(self.host.document());
        let default_name = format!("{}-bom.csv", slugify(&self.host.document().title));
        let Some(path) = dialogs::save_project("Export BOM as CSV", &default_name, "csv") else {
            return;
        };
        match std::fs::write(&path, content) {
            Ok(()) => self
                .shell
                .set_status(format!("Exported BOM to {}", path.display())),
            Err(error) => self
                .shell
                .notify(Tone::Error, "Could not export BOM", error.to_string()),
        }
    }

    fn export_svg(&mut self) {
        let content = logic::document_svg(self.host.document());
        let default_name = format!("{}.svg", slugify(&self.host.document().title));
        let Some(path) = dialogs::save_project("Export schematic as SVG", &default_name, "svg")
        else {
            return;
        };
        match std::fs::write(&path, content) {
            Ok(()) => self
                .shell
                .set_status(format!("Exported schematic to {}", path.display())),
            Err(error) => self
                .shell
                .notify(Tone::Error, "Could not export SVG", error.to_string()),
        }
    }

    fn request_new(&mut self) {
        self.finish_prop_gesture();
        if self.host.is_dirty() {
            self.pending = Some(PendingAction::New);
        } else {
            self.new_document();
        }
    }

    fn request_open(&mut self) {
        self.finish_prop_gesture();
        let Some(path) = dialogs::open_project("Open project", &["json"]) else {
            return;
        };
        if self.host.is_dirty() {
            self.pending = Some(PendingAction::Open(path));
        } else {
            self.load_path(path);
        }
    }

    fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::New => self.new_document(),
            PendingAction::Open(path) => self.load_path(path),
        }
    }

    fn resolve_pending(&mut self, choice: PendingChoice) {
        let Some(action) = self.pending.take() else {
            return;
        };
        match choice {
            PendingChoice::Cancel => {}
            PendingChoice::Discard => self.perform(action),
            PendingChoice::Save => {
                if self.save() {
                    self.perform(action);
                } else {
                    self.pending = Some(action);
                }
            }
        }
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        self.finish_prop_gesture();
        self.host.apply(|document| {
            document.remove_component(id);
        });
        self.selected = None;
        self.wire_start = None;
        self.shell.set_status("Component deleted");
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::New => self.request_new(),
            Action::Open => self.request_open(),
            Action::Save => {
                self.save();
            }
            Action::SaveAs => {
                self.save_as();
            }
            Action::ExportCsv => self.export_csv(),
            Action::ExportSvg => self.export_svg(),
            Action::Undo => {
                self.finish_prop_gesture();
                if self.host.undo() {
                    self.selected = None;
                    self.shell.set_status("Undo");
                }
            }
            Action::Redo => {
                self.finish_prop_gesture();
                if self.host.redo() {
                    self.selected = None;
                    self.shell.set_status("Redo");
                }
            }
            Action::Delete => self.delete_selected(),
            Action::About => self.shell.open_about(),
            Action::Settings => self.shell.open_settings(),
        }
    }

    fn handle_shortcuts(&mut self, ui: &egui::Ui) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let action = ui.input_mut(|input| {
            if input.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::S,
            ) {
                Some(Action::SaveAs)
            } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::S) {
                Some(Action::Save)
            } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::O) {
                Some(Action::Open)
            } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::N) {
                Some(Action::New)
            } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z) {
                Some(Action::Undo)
            } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y) {
                Some(Action::Redo)
            } else if input.key_pressed(egui::Key::Delete)
                || input.key_pressed(egui::Key::Backspace)
            {
                Some(Action::Delete)
            } else {
                None
            }
        });
        if let Some(action) = action {
            self.handle_action(action);
        }
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.armed_kind = None;
            self.wire_start = None;
            self.tool = Tool::Select;
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let title = self.host.display_name();
        let dirty = self.host.is_dirty();
        let can_undo = self.host.can_undo();
        let can_redo = self.host.can_redo();
        let mut action = None;
        self.shell.top_bar(ui, &title, dirty, |ui| {
            ui.menu_button("File", |ui| {
                for (label, candidate) in [
                    ("New", Action::New),
                    ("Open...", Action::Open),
                    ("Save", Action::Save),
                    ("Save As...", Action::SaveAs),
                ] {
                    if ui.button(label).clicked() {
                        action = Some(candidate);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Export BOM (CSV)...").clicked() {
                    action = Some(Action::ExportCsv);
                    ui.close();
                }
                if ui.button("Export Schematic (SVG)...").clicked() {
                    action = Some(Action::ExportSvg);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo"))
                    .clicked()
                {
                    action = Some(Action::Undo);
                    ui.close();
                }
                if ui
                    .add_enabled(can_redo, egui::Button::new("Redo"))
                    .clicked()
                {
                    action = Some(Action::Redo);
                    ui.close();
                }
                if ui.button("Delete selection").clicked() {
                    action = Some(Action::Delete);
                    ui.close();
                }
            });
            if ui.button("About").clicked() {
                action = Some(Action::About);
            }
            if ui.button("Settings").clicked() {
                action = Some(Action::Settings);
            }
        });
        if let Some(action) = action {
            self.handle_action(action);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let components = self.host.document().components.len();
        let wires = self.host.document().wires.len();
        self.shell.status_bar_with(ui, |ui| {
            ui.label(
                RichText::new(format!("{components} components · {wires} wires"))
                    .size(10.5)
                    .color(theme::FAINT),
            );
        });
    }

    fn palette_panel(&mut self, ui: &mut egui::Ui) {
        let mut armed = None;
        egui::Panel::left("circuit-palette")
            .resizable(true)
            .default_size(190.0)
            .min_size(150.0)
            .show(ui, |ui| {
                widgets::section_label(ui, "Components");
                ui.add_space(4.0);
                for kind in ComponentKind::ALL {
                    let selected = self.armed_kind == Some(kind);
                    let text = if selected {
                        RichText::new(kind.name())
                            .strong()
                            .color(theme::ACCENT_LIGHT)
                    } else {
                        RichText::new(kind.name())
                    };
                    if ui
                        .add(
                            egui::Button::new(text)
                                .fill(theme::PANEL)
                                .corner_radius(egui::CornerRadius::same(5))
                                .min_size(Vec2::new(0.0, 28.0)),
                        )
                        .clicked()
                    {
                        armed = Some(kind);
                    }
                }
                ui.separator();
                ui.label(
                    RichText::new("Pick a component, then click the canvas to place it.")
                        .size(10.5)
                        .color(theme::FAINT),
                );
                ui.label(
                    RichText::new("Esc cancels placement or wiring.")
                        .size(10.5)
                        .color(theme::FAINT),
                );
            });
        if let Some(kind) = armed {
            self.finish_prop_gesture();
            self.armed_kind = Some(kind);
            self.tool = Tool::Select;
            self.wire_start = None;
            self.shell.set_status(format!("Placing {}", kind.name()));
        }
    }

    fn inspector_panel(&mut self, ui: &mut egui::Ui) {
        let mut gain = false;
        let mut lost = false;
        let mut delete = false;
        egui::Panel::right("circuit-inspector")
            .resizable(true)
            .default_size(260.0)
            .min_size(200.0)
            .show(ui, |ui| {
                widgets::section_label(ui, "Netlist");
                ui.add_space(4.0);
                let nets = logic::netlist(self.host.document());
                let labels: Vec<String> = nets
                    .iter()
                    .map(|net| net_label(self.host.document(), net))
                    .collect();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (net, label) in nets.iter().zip(labels) {
                            ui.label(
                                RichText::new(format!("Net {}", net.id))
                                    .size(11.0)
                                    .strong()
                                    .color(theme::ACCENT),
                            );
                            ui.label(RichText::new(label).size(10.5).color(theme::MUTED));
                            ui.add_space(2.0);
                        }
                    });
                ui.separator();
                widgets::section_label(ui, "Properties");
                ui.add_space(4.0);
                if let Some(id) = self.selected {
                    let exists = self
                        .host
                        .document()
                        .components
                        .iter()
                        .any(|component| component.id == id);
                    if !exists {
                        self.selected = None;
                    }
                }
                if let Some(id) = self.selected {
                    let (gained, lost_focus) = {
                        let component = self
                            .host
                            .history_mut()
                            .current_mut()
                            .components
                            .iter_mut()
                            .find(|component| component.id == id);
                        if let Some(component) = component {
                            ui.label(
                                RichText::new(format!("{} · id {id}", component.kind.name()))
                                    .size(10.5)
                                    .color(theme::MUTED),
                            );
                            let label_response = ui
                                .horizontal(|ui| {
                                    ui.label("Label");
                                    ui.text_edit_singleline(&mut component.label)
                                })
                                .inner;
                            let value_response = ui
                                .horizontal(|ui| {
                                    ui.label("Value");
                                    ui.text_edit_singleline(&mut component.value)
                                })
                                .inner;
                            (
                                label_response.gained_focus() || value_response.gained_focus(),
                                label_response.lost_focus() || value_response.lost_focus(),
                            )
                        } else {
                            (false, false)
                        }
                    };
                    gain = gained;
                    lost = lost_focus;
                    if widgets::toolbar_button(ui, "Delete component", "Remove it and its wires")
                        .clicked()
                    {
                        delete = true;
                    }
                } else {
                    ui.label(
                        RichText::new("Select a component to edit its label and value.")
                            .size(10.5)
                            .color(theme::FAINT),
                    );
                }
            });
        if gain && !self.prop_gesture_active {
            self.host.history_mut().begin_gesture();
            self.prop_gesture_active = true;
        }
        if lost && self.prop_gesture_active {
            self.host.history_mut().end_gesture();
            self.prop_gesture_active = false;
        }
        if delete {
            self.delete_selected();
        }
    }

    fn central_panel(&mut self, ui: &mut egui::Ui) {
        let mut fit = false;
        let mut tool = None;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Schematic");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::toolbar_button(ui, "Fit", "Center and scale the circuit").clicked()
                    {
                        fit = true;
                    }
                    if ui
                        .add(egui::Button::new("Wire").selected(self.tool == Tool::Wire))
                        .on_hover_text("Click two pins to connect them")
                        .clicked()
                    {
                        tool = Some(Tool::Wire);
                    }
                    if ui
                        .add(egui::Button::new("Select").selected(self.tool == Tool::Select))
                        .clicked()
                    {
                        tool = Some(Tool::Select);
                    }
                    ui.label(
                        RichText::new(format!("{}%", (self.zoom * 100.0).round() as i32))
                            .size(10.5)
                            .color(theme::MUTED),
                    );
                });
            });
            ui.add_space(4.0);
            ui.separator();
            let canvas_rect = ui.available_rect_before_wrap();
            self.paint_canvas(ui, canvas_rect);
        });
        if fit {
            self.needs_fit = true;
        }
        if let Some(tool) = tool {
            self.finish_prop_gesture();
            self.tool = tool;
            self.wire_start = None;
            if tool == Tool::Select {
                self.armed_kind = None;
            }
        }
    }

    fn paint_canvas(&mut self, ui: &mut egui::Ui, canvas_rect: Rect) {
        let mut pan = self.pan;
        let mut zoom = self.zoom;
        let to_local = |screen: Pos2| screen - canvas_rect.min;
        let to_world = |local: Vec2, pan: Vec2, zoom: f32| (local - pan) / zoom;
        let to_screen = |world: Vec2, pan: Vec2, zoom: f32| {
            Pos2::new(
                canvas_rect.min.x + world.x * zoom + pan.x,
                canvas_rect.min.y + world.y * zoom + pan.y,
            )
        };

        if self.needs_fit {
            let mut min = Vec2::splat(f32::INFINITY);
            let mut max = Vec2::splat(f32::NEG_INFINITY);
            for component in &self.host.document().components {
                min = min.min(Vec2::new(
                    component.x as f32 - 60.0,
                    component.y as f32 - 60.0,
                ));
                max = max.max(Vec2::new(
                    component.x as f32 + 60.0,
                    component.y as f32 + 60.0,
                ));
            }
            if min.x.is_finite() {
                let size = max - min;
                let available = canvas_rect.size();
                zoom = (available.x / (size.x + 80.0))
                    .min(available.y / (size.y + 80.0))
                    .clamp(0.25, 2.0);
                pan = Vec2::new(
                    available.x / 2.0 - (min.x + max.x) / 2.0 * zoom,
                    available.y / 2.0 - (min.y + max.y) / 2.0 * zoom,
                );
            }
            self.needs_fit = false;
        }

        if ui.input(|input| input.pointer.button_down(egui::PointerButton::Middle)) {
            pan += ui.input(|input| input.pointer.delta());
        }
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON
            && let Some(pointer) = ui.input(|input| input.pointer.latest_pos())
        {
            let local = to_local(pointer);
            let world = to_world(local, pan, zoom);
            zoom = (zoom * (1.0 + scroll * 0.002)).clamp(0.25, 3.0);
            pan = local - world * zoom;
        }

        let response = ui.allocate_rect(canvas_rect, egui::Sense::click_and_drag());
        let pointer_local = response.interact_pointer_pos().map(to_local);

        if response.drag_started()
            && let Some(local) = pointer_local
        {
            let hit = {
                let document = self.host.document();
                hit_test(document, local, pan, zoom)
            };
            if let Hit::Component(id) = hit {
                let (start_x, start_y) = self
                    .host
                    .document()
                    .components
                    .iter()
                    .find(|component| component.id == id)
                    .map_or((0.0, 0.0), |component| (component.x, component.y));
                self.finish_prop_gesture();
                self.selected = Some(id);
                self.drag = Some(DragState {
                    id,
                    start_world: to_world(local, pan, zoom),
                    start_x,
                    start_y,
                });
                self.host.history_mut().begin_gesture();
            }
        }

        if let Some(drag) = self.drag.clone()
            && ui.input(|input| input.pointer.primary_down())
            && let Some(pointer) = ui.input(|input| input.pointer.latest_pos())
        {
            let world = to_world(to_local(pointer), pan, zoom);
            let (dx, dy) = (world.x - drag.start_world.x, world.y - drag.start_world.y);
            if let Some(component) = self
                .host
                .history_mut()
                .current_mut()
                .components
                .iter_mut()
                .find(|component| component.id == drag.id)
            {
                component.x = drag.start_x + f64::from(dx);
                component.y = drag.start_y + f64::from(dy);
            }
        }
        if ui.input(|input| input.pointer.primary_released()) && self.drag.is_some() {
            self.host.history_mut().end_gesture();
            self.drag = None;
        }

        if response.clicked()
            && let Some(local) = pointer_local
        {
            let hit = {
                let document = self.host.document();
                hit_test(document, local, pan, zoom)
            };
            match (self.tool, self.armed_kind, hit) {
                (_, Some(kind), Hit::None) => {
                    let world = to_world(local, pan, zoom);
                    let mut placed = None;
                    self.host.apply(|document| {
                        placed = Some(document.add_component(
                            kind,
                            f64::from(world.x),
                            f64::from(world.y),
                        ));
                    });
                    self.finish_prop_gesture();
                    self.selected = placed;
                    self.armed_kind = None;
                    self.shell.set_status(format!("Placed {}", kind.name()));
                }
                (Tool::Wire, _, Hit::Pin(pin)) => {
                    if let Some(start) = self.wire_start.take() {
                        if start != pin {
                            self.host.apply(|document| {
                                document.add_wire(start.clone(), pin.clone());
                            });
                            self.shell.set_status("Wire connected");
                        }
                    } else {
                        self.wire_start = Some(pin);
                        self.shell.set_status("Select the second pin");
                    }
                }
                (Tool::Wire, _, _) => {
                    self.wire_start = None;
                }
                (Tool::Select, _, Hit::Component(id)) => {
                    self.finish_prop_gesture();
                    self.selected = Some(id);
                }
                (Tool::Select, _, Hit::Pin(pin)) => {
                    self.finish_prop_gesture();
                    self.selected = Some(pin.component);
                }
                (Tool::Select, _, Hit::None) => {
                    self.finish_prop_gesture();
                    self.selected = None;
                }
            }
        }

        self.pan = pan;
        self.zoom = zoom;
        let screen = |world: Vec2| to_screen(world, pan, zoom);
        self.draw_canvas(ui, canvas_rect, pan, zoom, screen, pointer_local);
    }

    fn draw_canvas(
        &self,
        ui: &egui::Ui,
        canvas_rect: Rect,
        pan: Vec2,
        zoom: f32,
        to_screen: impl Fn(Vec2) -> Pos2,
        pointer_local: Option<Vec2>,
    ) {
        let painter = ui.painter_at(canvas_rect);
        painter.rect_filled(canvas_rect, 0.0, theme::SUNKEN);

        let world_min = (Vec2::ZERO - pan) / zoom;
        let world_max = (canvas_rect.size() - pan) / zoom;
        let first_x = (world_min.x / GRID).floor() * GRID;
        let first_y = (world_min.y / GRID).floor() * GRID;
        let mut x = first_x;
        while x <= world_max.x {
            let screen = to_screen(Vec2::new(x, 0.0));
            painter.line_segment(
                [
                    Pos2::new(screen.x, canvas_rect.min.y),
                    Pos2::new(screen.x, canvas_rect.max.y),
                ],
                Stroke::new(1.0, theme::BORDER),
            );
            x += GRID;
        }
        let mut y = first_y;
        while y <= world_max.y {
            let screen = to_screen(Vec2::new(0.0, y));
            painter.line_segment(
                [
                    Pos2::new(canvas_rect.min.x, screen.y),
                    Pos2::new(canvas_rect.max.x, screen.y),
                ],
                Stroke::new(1.0, theme::BORDER),
            );
            y += GRID;
        }

        let document = self.host.document();
        for wire in &document.wires {
            let start = pin_world(document, &wire.a);
            let end = pin_world(document, &wire.b);
            if let (Some(a), Some(b)) = (start, end) {
                painter.line_segment(
                    [to_screen(a), to_screen(b)],
                    Stroke::new(2.5 * zoom, theme::INFO),
                );
            }
        }

        if self.tool == Tool::Wire
            && let (Some(start), Some(pointer)) = (self.wire_start.as_ref(), pointer_local)
            && let Some(position) = pin_world(document, start)
        {
            painter.line_segment(
                [to_screen(position), to_screen(pointer)],
                Stroke::new(1.5, theme::ACCENT_LIGHT),
            );
        }

        for component in &document.components {
            let selected = self.selected == Some(component.id);
            let center = to_screen(Vec2::new(component.x as f32, component.y as f32));
            if selected {
                let half = 46.0 * zoom;
                painter.rect_stroke(
                    Rect::from_center_size(center, Vec2::splat(half * 2.0)),
                    egui::CornerRadius::same(6),
                    Stroke::new(1.5, theme::ACCENT),
                    egui::StrokeKind::Outside,
                );
            }
            for shape in symbol_shapes(
                component.kind,
                component.x,
                component.y,
                &component.label,
                &component.value,
            ) {
                paint_symbol(&painter, &shape, &to_screen, zoom, selected);
            }
        }
    }

    fn unsaved_window(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() {
            return;
        }
        let mut open = true;
        let mut choice = None;
        egui::Window::new("Unsaved changes")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label("The current project has unsaved changes.");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Save and continue").clicked() {
                        choice = Some(PendingChoice::Save);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(PendingChoice::Discard);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(PendingChoice::Cancel);
                    }
                });
            });
        if let Some(choice) = choice {
            self.resolve_pending(choice);
        } else if !open {
            self.pending = None;
        }
    }
}

impl eframe::App for CircuitApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ui);
        self.top_bar(ui);
        self.status_bar(ui);
        self.palette_panel(ui);
        self.inspector_panel(ui);
        self.central_panel(ui);
        self.unsaved_window(ui.ctx());
        self.shell.show_windows(ui.ctx());
        self.shell
            .update_title(ui.ctx(), &self.host.display_name(), self.host.is_dirty());
    }
}

fn pin_world(document: &Document, pin: &Pin) -> Option<Vec2> {
    document
        .components
        .iter()
        .find(|component| component.id == pin.component)
        .and_then(|component| component.pin_position(&pin.name))
        .map(|(x, y)| Vec2::new(x as f32, y as f32))
}

fn hit_test(document: &Document, local: Vec2, pan: Vec2, zoom: f32) -> Hit {
    for component in &document.components {
        for name in component.kind.pins() {
            if let Some((x, y)) = component.pin_position(name) {
                let screen = Vec2::new(x as f32 * zoom + pan.x, y as f32 * zoom + pan.y);
                if (screen - local).length() <= 9.0 {
                    return Hit::Pin(Pin::new(component.id, *name));
                }
            }
        }
    }
    for component in &document.components {
        let center = Vec2::new(
            component.x as f32 * zoom + pan.x,
            component.y as f32 * zoom + pan.y,
        );
        let half = 34.0 * zoom;
        let min = center - Vec2::splat(half);
        let max = center + Vec2::splat(half);
        if min.x <= local.x && local.x <= max.x && min.y <= local.y && local.y <= max.y {
            return Hit::Component(component.id);
        }
    }
    Hit::None
}

fn paint_symbol(
    painter: &egui::Painter,
    shape: &SymbolShape,
    to_screen: &impl Fn(Vec2) -> Pos2,
    zoom: f32,
    selected: bool,
) {
    let stroke_color = if selected {
        theme::ACCENT_LIGHT
    } else {
        theme::TEXT
    };
    let point = |(x, y): (f64, f64)| to_screen(Vec2::new(x as f32, y as f32));
    match shape {
        SymbolShape::Line { a, b, width } => {
            painter.line_segment(
                [point(*a), point(*b)],
                Stroke::new((*width as f32 * zoom).max(1.0), stroke_color),
            );
        }
        SymbolShape::Circle {
            center,
            radius,
            filled,
        } => {
            let center = point(*center);
            let radius = *radius as f32 * zoom;
            if *filled {
                painter.circle_filled(center, radius, theme::ACCENT);
            } else {
                painter.circle_stroke(
                    center,
                    radius,
                    Stroke::new((2.0 * zoom).max(1.0), stroke_color),
                );
            }
        }
        SymbolShape::Poly {
            points,
            closed,
            width,
        } => {
            let points: Vec<Pos2> = points.iter().map(|point_| point(*point_)).collect();
            let stroke = Stroke::new((*width as f32 * zoom).max(1.0), stroke_color);
            for pair in points.windows(2) {
                painter.line_segment([pair[0], pair[1]], stroke);
            }
            if let (Some(&first), Some(&last)) = (points.first(), points.last())
                && *closed
                && points.len() > 2
            {
                painter.line_segment([last, first], stroke);
            }
        }
        SymbolShape::Text { pos, text, size } => {
            let font_size = (*size as f32 * zoom).clamp(7.0, 28.0);
            painter.text(
                point(*pos),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(font_size),
                stroke_color,
            );
        }
    }
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "untitled".to_owned()
    } else {
        slug.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_finds_pins_and_components() {
        let document = Document::sample();
        let pan = Vec2::ZERO;
        let zoom = 1.0;
        assert_eq!(
            hit_test(&document, Vec2::new(120.0, -40.0), pan, zoom),
            Hit::Pin(Pin::new(2, "1"))
        );
        assert_eq!(
            hit_test(&document, Vec2::new(160.0, -40.0), pan, zoom),
            Hit::Component(2)
        );
        assert_eq!(
            hit_test(&document, Vec2::new(-500.0, 0.0), pan, zoom),
            Hit::None
        );
    }

    #[test]
    fn slugify_makes_safe_file_names() {
        assert_eq!(slugify("CircuitCraft sample"), "circuitcraft-sample");
        assert_eq!(slugify("<weird>"), "weird");
        assert_eq!(slugify("___"), "untitled");
    }
}
