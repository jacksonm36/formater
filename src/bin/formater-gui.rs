//! Asztali felület: Word/Excel feldolgozás.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use eframe::egui;
use formater::{run_fix, FixResult, RunParams};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

fn default_dict_dir_string() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.join("dicts")))
        .filter(|p| p.exists())
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "dicts".to_string())
}

fn trim_path(s: &str) -> Option<PathBuf> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(PathBuf::from(t))
    }
}

struct FormaterApp {
    input_path: String,
    output_path: String,
    dict_dir: String,
    dic_hu: String,
    dic_en: String,
    enable_align: bool,
    enable_spell: bool,
    max_edit_distance: i64,
    docx_merge_runs: bool,
    show_git_diff: bool,
    learn_habits: bool,
    learn_path: String,
    #[cfg(feature = "hunspell")]
    native_hunspell: bool,
    status: String,
    diff_text: String,
    result_rx: Option<Receiver<Result<FixResult, String>>>,
}

impl FormaterApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Self {
            input_path: String::new(),
            output_path: String::new(),
            dict_dir: default_dict_dir_string(),
            dic_hu: String::new(),
            dic_en: String::new(),
            enable_align: true,
            enable_spell: true,
            max_edit_distance: 2,
            docx_merge_runs: false,
            show_git_diff: true,
            learn_habits: true,
            learn_path: String::new(),
            #[cfg(feature = "hunspell")]
            native_hunspell: false,
            status: String::new(),
            diff_text: String::new(),
            result_rx: None,
        }
    }

    fn build_params(&self) -> Result<RunParams, &'static str> {
        let input = PathBuf::from(self.input_path.trim());
        if input.as_os_str().is_empty() {
            return Err("Válassz bemeneti .docx vagy .xlsx fájlt.");
        }
        if !input.exists() {
            return Err("A bemeneti fájl nem létezik.");
        }
        let dict_dir = trim_path(&self.dict_dir).unwrap_or_else(|| PathBuf::from("dicts"));
        Ok(RunParams {
            input,
            output: trim_path(&self.output_path),
            dict_dir,
            dic_hu: trim_path(&self.dic_hu),
            dic_en: trim_path(&self.dic_en),
            enable_align: self.enable_align,
            enable_spell: self.enable_spell,
            max_edit_distance: self.max_edit_distance,
            docx_merge_all_runs: self.docx_merge_runs,
            git_diff: self.show_git_diff,
            learn_habits: self.learn_habits,
            learn_path: trim_path(&self.learn_path),
            #[cfg(feature = "hunspell")]
            native_hunspell: self.native_hunspell,
        })
    }

    fn start_job(&mut self) {
        self.status.clear();
        self.diff_text.clear();
        let params = match self.build_params() {
            Ok(p) => p,
            Err(e) => {
                self.status = e.to_string();
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        self.result_rx = Some(rx);
        std::thread::spawn(move || {
            let msg = run_fix(&params, true).map_err(|e| e.to_string());
            let _ = tx.send(msg);
        });
    }

    fn poll_job(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.result_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(r)) => {
                self.status = format!(
                    "Kész: {} ({} → {} bájt)",
                    r.output_path.display(),
                    r.input_bytes,
                    r.output_bytes
                );
                self.diff_text = r.unified_diff.unwrap_or_default();
                self.result_rx = None;
            }
            Ok(Err(e)) => {
                self.status = format!("Hiba: {e}");
                self.result_rx = None;
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(32));
            }
            Err(TryRecvError::Disconnected) => {
                self.status = "Belső hiba: a feldolgozó szál megszakadt.".to_string();
                self.result_rx = None;
            }
        }
    }

    fn pick_input_file(field: &mut String) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Office", &["docx", "xlsx"])
            .pick_file()
        {
            if let Some(s) = p.to_str() {
                *field = s.to_string();
            }
        }
    }

    fn pick_save_path(field: &mut String) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Office", &["docx", "xlsx"])
            .save_file()
        {
            if let Some(s) = p.to_str() {
                *field = s.to_string();
            }
        }
    }

    fn pick_folder(field: &mut String) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            if let Some(s) = p.to_str() {
                *field = s.to_string();
            }
        }
    }

    fn pick_dic(field: &mut String) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Hunspell", &["dic"])
            .pick_file()
        {
            if let Some(s) = p.to_str() {
                *field = s.to_string();
            }
        }
    }
}

impl eframe::App for FormaterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_job(ctx);
        let working = self.result_rx.is_some();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Formater");
            ui.label("Word (.docx) és Excel (.xlsx): mondat-igazítás és szótár.");
            ui.add_space(8.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Bemenet:");
                    ui.add_enabled_ui(!working, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.input_path)
                                .desired_width(280.0)
                                .hint_text("*.docx vagy *.xlsx"),
                        );
                    });
                    if ui
                        .add_enabled(!working, egui::Button::new("Tallózás…"))
                        .clicked()
                    {
                        FormaterApp::pick_input_file(&mut self.input_path);
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Kimenet:");
                    ui.add_enabled_ui(!working, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.output_path)
                                .desired_width(280.0)
                                .hint_text("üres = automatikus *.fixed.*"),
                        );
                    });
                    if ui
                        .add_enabled(!working, egui::Button::new("Mentés másként…"))
                        .clicked()
                    {
                        FormaterApp::pick_save_path(&mut self.output_path);
                    }
                });

                ui.label(
                    egui::RichText::new(
                        "Beépített angol + magyar alapszótár az exe-ben. A mappa és a .dic mezők opcionálisan bővítik.",
                    )
                    .size(11.0)
                    .color(egui::Color32::from_rgb(180, 190, 210)),
                );
                ui.horizontal(|ui| {
                    ui.label("Szótár mappa:");
                    ui.add_enabled_ui(!working, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.dict_dir).desired_width(220.0),
                        );
                    });
                    if ui
                        .add_enabled(!working, egui::Button::new("Mappa…"))
                        .clicked()
                    {
                        FormaterApp::pick_folder(&mut self.dict_dir);
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("hu .dic (opcionális):");
                    ui.add_enabled_ui(!working, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.dic_hu).desired_width(220.0));
                    });
                    if ui.add_enabled(!working, egui::Button::new("…")).clicked() {
                        FormaterApp::pick_dic(&mut self.dic_hu);
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("en .dic (opcionális):");
                    ui.add_enabled_ui(!working, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.dic_en).desired_width(220.0));
                    });
                    if ui.add_enabled(!working, egui::Button::new("…")).clicked() {
                        FormaterApp::pick_dic(&mut self.dic_en);
                    }
                });

                ui.add_space(6.0);
                ui.add_enabled_ui(!working, |ui| {
                    ui.checkbox(&mut self.enable_align, "Ismétlődő mondatok igazítása");
                    ui.checkbox(&mut self.enable_spell, "Helyesírás (szótár / SymSpell)");
                    ui.add_enabled_ui(self.enable_spell, |ui| {
                        ui.checkbox(
                            &mut self.learn_habits,
                            "Tanulás: ismételt javítások megjegyzése (learned_habits.json)",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Tanulási fájl (üres = alapértelmezés):");
                        ui.add_enabled_ui(self.enable_spell, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.learn_path)
                                    .desired_width(260.0),
                            );
                        });
                    });
                    ui.checkbox(
                        &mut self.docx_merge_runs,
                        "Word: összevonás egy szövegfutásba (régi mód)",
                    );
                    ui.checkbox(
                        &mut self.show_git_diff,
                        "Git-szerű diff: kinyert szöveg változásai (szóközök, sorok)",
                    );
                    #[cfg(feature = "hunspell")]
                    {
                        ui.checkbox(
                            &mut self.native_hunspell,
                            "Natív Hunspell (.aff+.dic) — csak hunspell feature fordítással",
                        );
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Max szerkesztési távolság:");
                    ui.add_enabled_ui(!working, |ui| {
                        ui.add(egui::Slider::new(&mut self.max_edit_distance, 1..=5));
                    });
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!working, egui::Button::new("Futtatás"))
                        .clicked()
                    {
                        self.start_job();
                    }
                    if working {
                        ui.spinner();
                        ui.label("Feldolgozás…");
                    }
                });

                if !self.status.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.label(
                        egui::RichText::new(&self.status)
                            .size(14.0)
                            .color(if self.status.starts_with("Hiba") {
                                egui::Color32::from_rgb(255, 120, 120)
                            } else {
                                egui::Color32::from_rgb(160, 255, 180)
                            }),
                    );
                }

                if !self.diff_text.is_empty() {
                    ui.add_space(8.0);
                    egui::CollapsingHeader::new("Változások (git diff, kinyert szöveg)")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(240.0)
                                .stick_to_bottom(false)
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(&self.diff_text)
                                            .font(egui::FontId::monospace(11.0))
                                            .color(egui::Color32::from_rgb(220, 225, 235)),
                                    );
                                });
                        });
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 720.0])
            .with_title("Formater"),
        ..Default::default()
    };
    eframe::run_native(
        "Formater",
        options,
        Box::new(|cc| Ok(Box::new(FormaterApp::new(cc)))),
    )
}
