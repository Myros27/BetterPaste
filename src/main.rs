#![windows_subsystem = "windows"]
#![allow(clippy::collapsible_if)]

use axum::{
    Router,
    extract::{Json, State},
    http::{Method, StatusCode},
    routing::post,
};
use eframe::egui;
use ignore::WalkBuilder;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc, time::Duration};
use tower_http::cors::{Any, CorsLayer};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AppConfig {
    port: u16,
    instructions: String,
    replacing_rules: String,
    example: String,
    about_content: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: 3030,
            instructions: "This file is a consolidated version of the codebase.\nThe organization of the content is as follows:\nOverview\nReplacingRules\nFileStructure\nFiles".to_string(),
            replacing_rules: "If the AI needs the content of a </Removed_By_Compression> Region, ask the user.\n\nSTRICT FORMATTING RULES:\n1. **DO NOT** put markdown code fences (```) *inside* the search/replace tags. It will cause a mismatch.\n2. **DO** wrap the ENTIRE block (from START to END) in a single code block for readability (e.g. ```rust).\n3. Whitespace Critical: The [<(x{SEARCH}x)>] block is used for an exact string match. You MUST copy the search text exactly from the source, preserving all indentation and newlines.".to_string(),
            example: "```rust\n[<(x{START}x)>]\nmesh_core/src/main.rs\n[<(x{SEARCH}x)>]\npub struct GuardResponse {\n    pub success: bool,\n    pub message: String,\n}\n[<(x{REPLACEWITH}x)>]\npub struct GuardResponse {\n    pub is_admin: bool,\n    pub success: bool,\n    pub message: String,\n}\n[<(x{END}x)>]\n```".to_string(),
            about_content: "# About BetterPaste\n\nBetterPaste is a tool to bridge your local codebase with AI Chat interfaces.\n# Made by\nMyros".to_string(),
        }
    }
}

fn load_config() -> AppConfig {
    let path = "betterPaste_config.json";
    if let Ok(content) = fs::read_to_string(path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        let cfg = AppConfig::default();
        save_config(&cfg);
        cfg
    }
}

fn save_config(cfg: &AppConfig) {
    let path = "betterPaste_config.json";
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(path, json);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
enum PatchStatus {
    Queued,
    Pending,
    Success,
    Failed(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct IncomingPatch {
    file_path: String,
    search_content: String,
    replace_content: String,
}

#[derive(Clone, Debug)]
struct PatchEntry {
    id: String,
    timestamp: String,
    data: IncomingPatch,
    status: PatchStatus,
    backup_content: Option<String>,
}

struct SharedAppState {
    patches: Vec<PatchEntry>,
    new_patch_alert: bool,
    is_paused: bool,
    auto_dismiss: bool,
    auto_apply: bool,
}

type SharedStateRef = Arc<Mutex<SharedAppState>>;

fn scan_files(root: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    // Filter out binaries and config
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name == "betterPaste_config.json" || file_name.ends_with(".exe") {
                            continue;
                        }
                    }
                    let path = entry
                        .path()
                        .strip_prefix(".")
                        .unwrap_or(entry.path())
                        .to_path_buf();
                    files.push(path);
                }
            }
            Err(err) => eprintln!("Error scanning file: {}", err),
        }
    }
    files.sort();
    files
}

fn compress_code(content: &str) -> String {
    let mut result = String::new();
    result.push_str("// <Removed_By_Compression> bodies hidden\n");
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub")
            || trimmed.starts_with("fn")
            || trimmed.starts_with("struct")
            || trimmed.starts_with("enum")
            || trimmed.starts_with("impl")
            || trimmed.starts_with("type")
            || trimmed.starts_with("use")
            || trimmed.starts_with("mod")
            || trimmed.starts_with("[")
            || trimmed.is_empty()
        {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

fn load_icon() -> egui::IconData {
    let icon_bytes = include_bytes!("../betterPaste.ico");
    let image = image::load_from_memory(icon_bytes)
        .expect("Failed to load icon")
        .to_rgba8();
    let (icon_width, icon_height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width: icon_width,
        height: icon_height,
    }
}

fn generate_xml(
    files: &[PathBuf],
    selected: &HashMap<PathBuf, bool>,
    partials: &HashMap<PathBuf, bool>,
    config: &AppConfig,
) -> String {
    let mut xml = String::new();

    xml.push_str("<Overview>\n<Instructions>\n");
    xml.push_str(&config.instructions);
    xml.push_str("\n</Instructions>\n</Overview>\n");

    xml.push_str("<ReplacingRules>\n<instructions>\n");
    xml.push_str(&config.replacing_rules);
    xml.push_str("\n</instructions>\n<Example>\n");
    xml.push_str(&config.example);
    xml.push_str("\n</Example>\n</ReplacingRules>\n");

    xml.push_str("<FileStructure>\n");
    for file in files {
        if *selected.get(file).unwrap_or(&false) {
            xml.push_str(&format!("{}\n", file.display()));
        }
    }
    xml.push_str("</FileStructure>\n<Files>\n");
    for file in files {
        if *selected.get(file).unwrap_or(&false) {
            let is_partial = *partials.get(file).unwrap_or(&false);
            if let Ok(content) = fs::read_to_string(file) {
                let final_content = if is_partial {
                    compress_code(&content)
                } else {
                    content
                };
                xml.push_str(&format!(
                    "<File path=\"{}\" compressed=\"{}\">\n",
                    file.display(),
                    is_partial
                ));
                xml.push_str(&final_content);
                xml.push_str("\n</File>\n");
            }
        }
    }
    xml.push_str("</Files>");
    xml
}

fn apply_patch(patch: &mut PatchEntry) {
    let path = PathBuf::from(&patch.data.file_path);
    match fs::read_to_string(&path) {
        Ok(raw_content) => {
            let content = raw_content.replace("\r\n", "\n");
            let search_norm = patch.data.search_content.replace("\r\n", "\n");
            let replace_norm = patch.data.replace_content.replace("\r\n", "\n");

            if content.contains(&search_norm) {
                patch.backup_content = Some(raw_content);
                let new_content = content.replace(&search_norm, &replace_norm);
                if let Err(e) = fs::write(&path, new_content) {
                    patch.status = PatchStatus::Failed(format!("IO Error: {}", e));
                } else {
                    patch.status = PatchStatus::Success;
                }
            } else {
                patch.status = PatchStatus::Failed(
                    "Search text not found (Check tabs/whitespace)".to_string(),
                );
            }
        }
        Err(e) => {
            patch.status = PatchStatus::Failed(format!("File missing: {}", e));
        }
    }
}

fn undo_patch(patch: &mut PatchEntry) {
    if let Some(backup) = &patch.backup_content {
        let path = PathBuf::from(&patch.data.file_path);
        if let Err(e) = fs::write(path, backup) {
            patch.status = PatchStatus::Failed(format!("Undo failed: {}", e));
        } else {
            patch.status = PatchStatus::Pending;
            patch.backup_content = None;
        }
    }
}

async fn diff_handler(
    State(state): State<SharedStateRef>,
    Json(payload): Json<IncomingPatch>,
) -> StatusCode {
    let mut app_state = state.lock();

    if app_state.auto_dismiss {
        println!("Auto-dismissed patch for {}", payload.file_path);
        return StatusCode::OK;
    }

    let mut entry = PatchEntry {
        id: format!("{}", chrono::Utc::now().timestamp_micros()),
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        data: payload,
        status: PatchStatus::Pending,
        backup_content: None,
    };

    if app_state.is_paused {
        entry.status = PatchStatus::Queued;
    } else if app_state.auto_apply {
        apply_patch(&mut entry);
    } else {
        entry.status = PatchStatus::Pending;
    }

    app_state.patches.push(entry);
    app_state.new_patch_alert = true;

    StatusCode::OK
}

async fn run_server(state: SharedStateRef, port: u16) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(vec![Method::POST])
        .allow_headers(Any);
    let app = Router::new()
        .route("/api/diff", post(diff_handler))
        .with_state(state)
        .layer(cors);
    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Server listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

struct BetterPasteApp {
    state: SharedStateRef,
    config: AppConfig,
    available_files: Vec<PathBuf>,
    selected_files: HashMap<PathBuf, bool>,
    partial_files: HashMap<PathBuf, bool>,
    generated_output: String,
    current_tab: AppTab,

    // UI State for Patcher
    expanded_patch_id: Option<String>,
    last_patch_count: usize,
    manual_patch_input: String, // For manual pasting
}

#[derive(PartialEq)]
enum AppTab {
    Generator,
    Patcher,
    Ungenerator,
    Configuration,
    About,
    Help,
}

impl BetterPasteApp {
    fn new(_cc: &eframe::CreationContext, state: SharedStateRef, config: AppConfig) -> Self {
        let mut app = Self {
            state,
            config,
            available_files: Vec::new(),
            selected_files: HashMap::new(),
            partial_files: HashMap::new(),
            generated_output: String::new(),
            current_tab: AppTab::Generator,
            expanded_patch_id: None,
            last_patch_count: 0,
            manual_patch_input: String::new(),
        };
        app.rescan();
        app
    }

    fn rescan(&mut self) {
        self.available_files = scan_files(".");
        if self.selected_files.is_empty() {
            for f in &self.available_files {
                self.selected_files.insert(f.clone(), true);
            }
        }
    }

    fn unpause_queue(&self) {
        let mut state = self.state.lock();
        state.is_paused = false;
        let auto_apply = state.auto_apply;

        for patch in state.patches.iter_mut() {
            if let PatchStatus::Queued = patch.status {
                if auto_apply {
                    apply_patch(patch);
                } else {
                    patch.status = PatchStatus::Pending;
                }
            }
        }
    }
}

impl eframe::App for BetterPasteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(500));

        {
            let state = self.state.lock();
            if state.patches.len() > self.last_patch_count {
                if let Some(last) = state.patches.last() {
                    self.expanded_patch_id = Some(last.id.clone());
                }
                self.last_patch_count = state.patches.len();
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("BetterPaste");
                ui.separator();
                if ui
                    .selectable_label(self.current_tab == AppTab::Generator, "Generator")
                    .clicked()
                {
                    self.current_tab = AppTab::Generator;
                }
                let alert = { self.state.lock().new_patch_alert };
                let btn_text = if alert { "🔴 Patcher" } else { "Patcher" };
                if ui
                    .selectable_label(self.current_tab == AppTab::Patcher, btn_text)
                    .clicked()
                {
                    self.current_tab = AppTab::Patcher;
                    self.state.lock().new_patch_alert = false;
                }
                if ui
                    .selectable_label(self.current_tab == AppTab::Ungenerator, "Ungenerator")
                    .clicked()
                {
                    self.current_tab = AppTab::Ungenerator;
                }

                ui.separator();

                if ui
                    .selectable_label(self.current_tab == AppTab::Configuration, "Configuration")
                    .clicked()
                {
                    self.current_tab = AppTab::Configuration;
                }
                if ui
                    .selectable_label(self.current_tab == AppTab::Help, "Help")
                    .clicked()
                {
                    self.current_tab = AppTab::Help;
                }
                if ui
                    .selectable_label(self.current_tab == AppTab::About, "About")
                    .clicked()
                {
                    self.current_tab = AppTab::About;
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.current_tab {
            AppTab::Generator => self.ui_generator(ui),
            AppTab::Patcher => self.ui_patcher(ui),
            AppTab::Ungenerator => self.ui_ungenerator(ui),
            AppTab::Configuration => self.ui_config(ui),
            AppTab::About => self.ui_about(ui),
            AppTab::Help => self.ui_help(ui),
        });
    }
}

impl BetterPasteApp {
    fn ui_about(&mut self, ui: &mut egui::Ui) {
        ui.heading("About BetterPaste");
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.config.about_content)
                    .desired_width(f32::INFINITY)
                    .frame(false)
                    .interactive(false),
            );
        });
    }

    fn ui_ungenerator(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Extract (Safe Mode)").clicked() {
                let re = regex::Regex::new(
                    r#"(?ms)<File path="([^"]+)"(?: compressed="[^"]+")?>\s*(.*?)\s*</File>"#,
                )
                .unwrap();
                for caps in re.captures_iter(&self.generated_output) {
                    if let (Some(path_match), Some(content_match)) = (caps.get(1), caps.get(2)) {
                        let path = PathBuf::from(path_match.as_str());
                        if !path.exists() {
                            if let Some(parent) = path.parent() {
                                let _ = fs::create_dir_all(parent);
                            }
                            let _ = fs::write(path, content_match.as_str().trim());
                        }
                    }
                }
            }
            ui.label("Files marked in RED already exist and are skipped.");
        });

        ui.separator();

        ui.columns(2, |columns| {
            // Left: Preview / Analysis
            columns[0].vertical(|ui| {
                ui.heading("Preview Analysis");
                egui::ScrollArea::vertical()
                    .id_salt("ungenerator_preview")
                    .show(ui, |ui| {
                        let re = regex::Regex::new(
                            r#"(?ms)<File path="([^"]+)"(?: compressed="[^"]+")?>"#,
                        )
                        .unwrap();
                        for caps in re.captures_iter(&self.generated_output) {
                            if let Some(path_match) = caps.get(1) {
                                let path = PathBuf::from(path_match.as_str());
                                let exists = path.exists();
                                ui.horizontal(|ui| {
                                    if exists {
                                        ui.colored_label(
                                            egui::Color32::RED,
                                            format!("EXISTS: {}", path.display()),
                                        );
                                    } else {
                                        ui.colored_label(
                                            egui::Color32::GREEN,
                                            format!("NEW: {}", path.display()),
                                        );
                                    }
                                });
                            }
                        }
                    });
            });

            // Right: Input
            columns[1].vertical(|ui| {
                ui.heading("XML Input");
                egui::ScrollArea::vertical()
                    .id_salt("ungenerator_input")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.generated_output)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(30),
                        );
                    });
            });
        });
    }
    fn ui_config(&mut self, ui: &mut egui::Ui) {
        ui.heading("Configuration");
        ui.add_space(10.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.group(|ui| {
                ui.label("Server Port (Default: 3030):");
                ui.add(egui::DragValue::new(&mut self.config.port).range(1024..=65535));
            });

            ui.add_space(10.0);

            ui.group(|ui| {
                ui.label("Overview Instructions:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.config.instructions)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
            });

            ui.add_space(10.0);

            ui.group(|ui| {
                ui.label("Replacing Rules Instructions:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.config.replacing_rules)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
            });

            ui.add_space(10.0);

            ui.group(|ui| {
                ui.label("Example Block (Sent to AI):");
                ui.label(
                    egui::RichText::new(
                        "This shows the AI how to format the response. Note the outer code fences.",
                    )
                    .size(10.0)
                    .weak(),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.config.example)
                        .code_editor()
                        .desired_rows(12)
                        .desired_width(f32::INFINITY),
                );
            });

            ui.add_space(15.0);

            if ui.button("💾 Save Configuration").clicked() {
                save_config(&self.config);
            }
        });
    }

    fn ui_help(&mut self, ui: &mut egui::Ui) {
        ui.heading("Help & Setup");

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("1. Browser Setup").strong().size(16.0));
            ui.label("To allow the AI to communicate with BetterPaste, you need a userscript manager.");
            ui.label("Recommended: Tampermonkey or Violentmonkey.");
            ui.label("1. Install the extension for Chrome/Firefox.");
            ui.label("2. Create a new script.");
            ui.label("3. Paste the code below and save.");
            ui.label("4. When prompted, allow the script to access '127.0.0.1'.");

            ui.add_space(5.0);

            ui.collapsing("Show Userscript", |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Copy Script to Clipboard").clicked() {
                        if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(TAMPERMONKEY_SCRIPT); }
                    }
                });

                let mut script_display = TAMPERMONKEY_SCRIPT.to_string();
                ui.add(egui::TextEdit::multiline(&mut script_display).code_editor().desired_width(f32::INFINITY).desired_rows(15).interactive(false));
            });

            ui.add_space(20.0);

            ui.label(egui::RichText::new("2. Workflow").strong().size(16.0));
            ui.label("1. Go to the 'Generator' tab.");
            ui.label("2. Select files to include (use 'Partial' for large files to hide function bodies).");
            ui.label("3. Click 'Generate XML' and copy to clipboard.");
            ui.label("4. Paste into your AI chat.");
            ui.label("5. When the AI responds with code blocks, they will appear in the 'Patcher' tab.");
            ui.label("6. Review and Apply changes.");

            ui.add_space(20.0);

            ui.label(egui::RichText::new("3. The Ungenerator").strong().size(16.0));
            ui.label("Paste a context XML file into the right panel to unpack it into files.");
            ui.label("Useful for bootstrapping projects from AI generated XML.");
            ui.label("Files marked in RED already exist and will be skipped.");

            ui.add_space(20.0);

            ui.label(egui::RichText::new("4. Configuration").strong().size(16.0));
            ui.label("Changes are saved to 'betterPaste_config.json' automatically.");
            ui.label("Note: Restart is required for Server Port changes to take effect.");
        });
    }

    fn ui_generator(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Rescan Directory").clicked() {
                self.rescan();
            }
            if ui.button("Generate XML").clicked() {
                self.generated_output = generate_xml(
                    &self.available_files,
                    &self.selected_files,
                    &self.partial_files,
                    &self.config,
                );
            }
        });
        ui.separator();
        ui.columns(2, |columns| {
            columns[0].vertical(|ui| {
                ui.heading("Files");
                egui::ScrollArea::vertical()
                    .id_salt("file_list")
                    .show(ui, |ui| {
                        for file in &self.available_files {
                            ui.horizontal(|ui| {
                                let mut is_sel = *self.selected_files.get(file).unwrap_or(&false);
                                if ui.checkbox(&mut is_sel, file.to_string_lossy()).changed() {
                                    self.selected_files.insert(file.clone(), is_sel);
                                }
                                if is_sel {
                                    let mut is_part =
                                        *self.partial_files.get(file).unwrap_or(&false);
                                    if ui.checkbox(&mut is_part, "Partial").changed() {
                                        self.partial_files.insert(file.clone(), is_part);
                                    }
                                }
                            });
                        }
                    });
            });
            columns[1].vertical(|ui| {
                ui.heading("Context Output");
                ui.horizontal(|ui| {
                    if ui.button("Copy to Clipboard (Formatted)").clicked() {
                        // Wraps in xml code block to preserve whitespace in AI
                        let formatted = format!("```xml\n{}\n```", self.generated_output);
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(formatted);
                        }
                    }

                    if ui.button("Save to File...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("context.xml")
                            .save_file()
                        {
                            let _ = fs::write(path, &self.generated_output);
                        }
                    }
                });

                egui::ScrollArea::vertical()
                    .id_salt("output_text")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.generated_output)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(30),
                        );
                    });
            });
        });
    }

    fn ui_patcher(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Manual Patch Input", |ui| {
            ui.label("Paste a [<(x{START}x)>] block here if the script misses it.");
            ui.add(egui::TextEdit::multiline(&mut self.manual_patch_input).code_editor().desired_rows(3).desired_width(f32::INFINITY));
            if ui.button("Process Manual Input").clicked() {
                // Send to local server virtually
                let re = regex::Regex::new(r"\[<\(x\{START\}x\)>\]\s*([\s\S]*?)\s*\[<\(x\{SEARCH\}x\)>\]\s*([\s\S]*?)\s*\[<\(x\{REPLACEWITH\}x\)>\]\s*([\s\S]*?)\s*\[<\(x\{END\}x\)>\]").unwrap();
                if let Some(caps) = re.captures(&self.manual_patch_input) {
                    let patch = IncomingPatch {
                        file_path: caps[1].trim().to_string(),
                        search_content: caps[2].to_string(),
                        replace_content: caps[3].to_string(),
                    };

                    let mut state = self.state.lock();
                    let mut entry = PatchEntry {
                        id: format!("{}", chrono::Utc::now().timestamp_micros()),
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        data: patch,
                        status: PatchStatus::Pending,
                        backup_content: None,
                    };
                    if !state.is_paused { apply_patch(&mut entry); }
                    else { entry.status = PatchStatus::Queued; }
                    
                    state.patches.push(entry);
                    self.manual_patch_input.clear();
                }
            }
        });
        ui.separator();

        ui.horizontal(|ui| {
            ui.heading("Incoming Patches");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Auto Dismiss Switch
                let mut auto_dismiss = { self.state.lock().auto_dismiss };
                if ui
                    .checkbox(&mut auto_dismiss, "Auto-Dismiss (Reload Protection)")
                    .changed()
                {
                    self.state.lock().auto_dismiss = auto_dismiss;
                }

                ui.separator();

                let mut is_paused = { self.state.lock().is_paused };
                let pause_text = if is_paused {
                    "▶ Resume & Apply Queue"
                } else {
                    "⏸ Pause"
                };
                if ui.toggle_value(&mut is_paused, pause_text).changed() {
                    if !is_paused {
                        self.unpause_queue();
                    } else {
                        self.state.lock().is_paused = true;
                    }
                }

                ui.separator();

                let mut auto_apply = { self.state.lock().auto_apply };
                if ui.checkbox(&mut auto_apply, "Auto-Apply").changed() {
                    self.state.lock().auto_apply = auto_apply;
                }
            });
        });
        ui.separator();

        let mut state = self.state.lock();
        let mut index_to_remove = None;

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {

                for (i, patch) in state.patches.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                let is_expanded = self.expanded_patch_id.as_ref() == Some(&patch.id);
                                let icon = if is_expanded { "▼" } else { "▶" };
                                if ui.button(icon).clicked() {
                                    if is_expanded {
                                        self.expanded_patch_id = None;
                                    } else {
                                        self.expanded_patch_id = Some(patch.id.clone());
                                    }
                                }

                                match &patch.status {
                                    PatchStatus::Queued => {
                                        if ui.button("Apply Now").clicked() { apply_patch(patch); }
                                    },
                                    PatchStatus::Success => {
                                        if ui.button("Undo").clicked() { undo_patch(patch); }
                                    },
                                    PatchStatus::Pending => {
                                        if ui.button("Apply").clicked() { apply_patch(patch); }
                                    },
                                    PatchStatus::Failed(_) => {
                                        if ui.button("Retry").clicked() { apply_patch(patch); }
                                    }
                                }

                                if ui.button("✖").on_hover_text("Dismiss").clicked() {
                                    index_to_remove = Some(i);
                                }

                                ui.separator();

                                match &patch.status {
                                    PatchStatus::Queued => ui.colored_label(egui::Color32::GRAY, "QUEUED"),
                                    PatchStatus::Pending => ui.colored_label(egui::Color32::YELLOW, "PENDING"),
                                    PatchStatus::Success => ui.colored_label(egui::Color32::GREEN, "SUCCESS"),
                                    PatchStatus::Failed(_) => ui.colored_label(egui::Color32::RED, "FAILED"),
                                };

                                ui.label(egui::RichText::new(&patch.data.file_path).strong());

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(egui::RichText::new(&patch.timestamp).weak());
                                });
                            });

                            if self.expanded_patch_id.as_ref() == Some(&patch.id) {
                                ui.separator();
                                if let PatchStatus::Failed(err) = &patch.status {
                                    ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
                                    if ui.button("Copy Error Report for AI").clicked() {
                                        let report = format!(
                                            "The following replace could not be found:\n\n[<(x{{START}}x)>]\n{}\n[<(x{{SEARCH}}x)>]\n{}\n[<(x{{REPLACEWITH}}x)>]\n{}\n[<(x{{END}}x)>]\n\nPlease check indentation/tabs.",
                                            patch.data.file_path, patch.data.search_content, patch.data.replace_content
                                        );
                                        if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(report); }
                                    }
                                }

                                ui.columns(2, |cols| {
                                    cols[0].label("Search:");
                                    cols[0].add(egui::TextEdit::multiline(&mut patch.data.search_content.as_str()).code_editor().interactive(false));
                                    cols[1].label("Replace:");
                                    cols[1].add(egui::TextEdit::multiline(&mut patch.data.replace_content.as_str()).code_editor().interactive(false));
                                });
                            }
                        });
                    });
                    ui.add_space(2.0);
                }
                if let Some(i) = index_to_remove {
                    state.patches.remove(i);
                }
            });
    }
}

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let config = load_config();
    let port = config.port;

    let state = Arc::new(Mutex::new(SharedAppState {
        patches: Vec::new(),
        new_patch_alert: false,
        is_paused: false,
        auto_dismiss: false,
        auto_apply: true,
    }));

    let server_state = state.clone();
    tokio::spawn(async move {
        run_server(server_state, port).await;
    });

    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    options.viewport.icon = Some(std::sync::Arc::new(load_icon()));
    eframe::run_native(
        "BetterPaste",
        options,
        Box::new(|cc| Ok(Box::new(BetterPasteApp::new(cc, state, config)))),
    )
}
#[rustfmt::skip]
const TAMPERMONKEY_SCRIPT: &str = r#"
// ==UserScript==
// @name         BetterPaste Connector
// @namespace    http://tampermonkey.net/
// @version      1.4
// @description  Scans AI chat for BetterPaste code blocks
// @match        https://chatgpt.com/*
// @match        https://gemini.google.com/*
// @match        https://claude.ai/*
// @match        https://chat.deepseek.com/*
// @match        https://aistudio.google.com/*
// @connect      127.0.0.1
// @grant        GM_xmlhttpRequest
// @run-at       document-idle
// ==/UserScript==

(function() {
    'use strict';

    const SERVER_URL = "http://127.0.0.1:3030/api/diff";
    const SCAN_INTERVAL_MS = 1000;

    let isScanning = false; // Start Paused
    let cornerIndex = 0; // 0=BR, 1=BL, 2=TL, 3=TR

    const uiContainer = document.createElement('div');
    uiContainer.style.cssText = 'position:fixed; z-index:9999; display:flex; align-items:center; gap:8px; padding:6px 10px; background:#222; border:1px solid #444; color:#fff; border-radius:6px; font-family:sans-serif; font-size:12px; box-shadow:0 4px 6px rgba(0,0,0,0.3); transition:all 0.3s ease;';

    const statusText = document.createElement('span');
    statusText.innerText = "BP: Paused";
    statusText.style.fontWeight = "bold";
    statusText.style.minWidth = "70px";

    const toggleBtn = document.createElement('button');
    toggleBtn.innerText = "▶";
    toggleBtn.style.cssText = 'background:#444; color:white; border:none; padding:4px 8px; border-radius:4px; cursor:pointer; font-size:12px;';

    const moveBtn = document.createElement('button');
    moveBtn.innerText = "✥";
    moveBtn.style.cssText = 'background:#444; color:white; border:none; padding:4px 8px; border-radius:4px; cursor:pointer; font-size:12px;';

    uiContainer.appendChild(statusText);
    uiContainer.appendChild(toggleBtn);
    uiContainer.appendChild(moveBtn);
    document.body.appendChild(uiContainer);

    const applyPosition = () => {
        uiContainer.style.top = uiContainer.style.bottom = uiContainer.style.left = uiContainer.style.right = 'auto';
        const margin = '15px';
        if (cornerIndex === 0) { uiContainer.style.bottom = margin; uiContainer.style.right = margin; }
        else if (cornerIndex === 1) { uiContainer.style.bottom = margin; uiContainer.style.left = margin; }
        else if (cornerIndex === 2) { uiContainer.style.top = margin; uiContainer.style.left = margin; }
        else if (cornerIndex === 3) { uiContainer.style.top = margin; uiContainer.style.right = margin; }
    };
    applyPosition();

    toggleBtn.onclick = () => {
        isScanning = !isScanning;
        if (isScanning) {
            toggleBtn.innerText = "⏸";
            statusText.innerText = "BP: Idle";
            statusText.style.color = " #fff";
            scanForBlocks();
        } else {
            toggleBtn.innerText = "▶";
            statusText.innerText = "BP: Paused";
            statusText.style.color = " #aaa";
        }
    };

    moveBtn.onclick = () => { cornerIndex = (cornerIndex + 1) % 4; applyPosition(); };

    const BLOCK_REGEX = /\[<\(x\{START\}x\)>\]\s*([\s\S]*?)\s*\[<\(x\{SEARCH\}x\)>\]\s*([\s\S]*?)\s*\[<\(x\{REPLACEWITH\}x\)>\]\s*([\s\S]*?)\s*\[<\(x\{END\}x\)>\]/g;

    function updateStatus(msg, color = null) {
        if (!isScanning) return;
        statusText.innerText = msg;
        if (color) statusText.style.color = color;
    }

    function scanForBlocks() {
        if (!isScanning) return;
        const bodyText = document.body.innerText;

        BLOCK_REGEX.lastIndex = 0;
        let match;

        while ((match = BLOCK_REGEX.exec(bodyText)) !== null) {
            const fullMatch = match[0];
            const filePath = match[1].trim();
            const searchBlock = match[2];
            const replaceBlock = match[3];
            const normalizedContent = fullMatch.replace(/\s/g, '');
            const blockHash = cyrb53(normalizedContent);

            if (sessionStorage.getItem(`bp_sent_${blockHash}`)) continue;

            if (searchBlock.length > 60 && !searchBlock.includes('\n')) {
                console.warn(`[BetterPaste] Skipping suspicious flattened block for ${filePath}`);
                continue;
            }

            updateStatus(`Sending...`, ' #e67e22');

            const payload = JSON.stringify({
                file_path: filePath,
                search_content: searchBlock,
                replace_content: replaceBlock
            });

            GM_xmlhttpRequest({
                method: "POST",
                url: SERVER_URL,
                headers: { "Content-Type": "application/json" },
                data: payload,
                onload: function(res) {
                    if (res.status >= 200 && res.status < 300) {
                        sessionStorage.setItem(`bp_sent_${blockHash}`, "true");
                        updateStatus("Synced", '#27ae60');
                        setTimeout(() => updateStatus("BP: Idle", ' #fff'), 2000);
                    } else {
                        updateStatus("Err: Backend", ' #c0392b');
                    }
                },
                onerror: function() {
                    updateStatus("Err: Connect", ' #c0392b');
                }
            });
        }
    }

    const cyrb53 = function(str, seed = 0) {
        let h1 = 0xdeadbeef ^ seed, h2 = 0x41c6ce57 ^ seed;
        for (let i = 0, ch; i < str.length; i++) {
            ch = str.charCodeAt(i);
            h1 = Math.imul(h1 ^ ch, 2654435761);
            h2 = Math.imul(h2 ^ ch, 1597334677);
        }
        h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
        h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
        return 4294967296 * (2097151 & h2) + (h1 >>> 0);
    };

    setInterval(scanForBlocks, SCAN_INTERVAL_MS);
})();
"#;
