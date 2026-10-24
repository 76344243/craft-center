//! Craft-Zentrale: zeigt für jede angepasste ArtCraft-App den Stand des Forks auf GitHub, die
//! installierte Version und ob beide übereinstimmen, und installiert Updates mit einem Klick.

mod core;

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, RichText};

use crate::core::{APPS, AppSpec, ForkState, Installed, Match, Release, SELF_NAME, Step};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Craft-Zentrale")
            .with_inner_size([780.0, 700.0])
            .with_min_inner_size([560.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Craft-Zentrale",
        options,
        Box::new(|cc| Ok(Box::new(Center::new(&cc.egui_ctx)))),
    )
}

/// One app's row.
struct Row {
    spec: AppSpec,
    fork: Option<ForkState>,
    installed: Option<Installed>,
    job: Option<Arc<Mutex<Step>>>,
    /// The app was quit for the update and is started again when it succeeds.
    relaunch: bool,
    /// The app is running (checked every two seconds, not every frame).
    running: bool,
}

struct Center {
    rows: Vec<Row>,
    /// Fork answers arriving from the background check: (row, state).
    incoming: Arc<Mutex<Vec<(usize, ForkState)>>>,
    checking: usize,
    checked_at: Option<chrono::DateTime<chrono::Local>>,
    auto_updates: bool,
    running_checked: Option<std::time::Instant>,
}

impl Center {
    fn new(ctx: &egui::Context) -> Self {
        let mut center = Center {
            rows: APPS
                .iter()
                .map(|spec| Row {
                    spec: spec.clone(),
                    fork: None,
                    installed: None,
                    job: None,
                    relaunch: false,
                    running: false,
                })
                .collect(),
            incoming: Arc::new(Mutex::new(Vec::new())),
            checking: 0,
            checked_at: None,
            auto_updates: core::auto_updates_on(),
            running_checked: None,
        };
        if std::env::var_os("CRAFT_CENTER_DEMO").is_some() {
            center.demo();
        } else {
            center.refresh(ctx);
        }
        center
    }

    /// Sample data for looking at the window without GitHub or installed apps (development).
    fn demo(&mut self) {
        use crate::core::Drift;
        let rel = |repo: &str, tag: &str| Release {
            tag: tag.into(),
            published: Some("2026-10-24T05:31:58Z".into()),
            page: format!("https://github.com/76344243/{repo}/releases/tag/{tag}"),
            dmg: Some("https://example.invalid/x.dmg".into()),
        };
        let inst = |name: &str, tag: Option<&str>, v: &str| Installed {
            path: std::path::PathBuf::from(format!("/Applications/{name}.app")),
            version: Some(v.into()),
            tag: tag.map(Into::into),
            when: Some(chrono::Local::now()),
        };
        let r0 = &mut self.rows[0];
        r0.fork = Some(ForkState {
            release: Ok(rel("wordcraft", "mein-stand-20261024-2")),
            drift: Some(Ok(Drift {
                behind: 0,
                ahead: 2,
            })),
        });
        r0.installed = Some(inst("WordCraft", Some("mein-stand-20261024-2"), "0.10.0"));
        let r1 = &mut self.rows[1];
        r1.fork = Some(ForkState {
            release: Ok(rel("pdfcraft", "mein-stand-20261024-3")),
            drift: Some(Ok(Drift {
                behind: 14,
                ahead: 3,
            })),
        });
        r1.installed = Some(inst("PdfCraft", Some("mein-stand-20261023-2"), "0.13.0"));
        r1.job = Some(Arc::new(Mutex::new(Step::Downloading {
            done: 17_400_000,
            total: Some(39_217_829),
        })));
        let r2 = &mut self.rows[2];
        r2.fork = Some(ForkState {
            release: Ok(rel("craft-center", "mein-stand-20261024-1")),
            drift: None,
        });
        r2.installed = Some(inst("CraftCenter", None, "0.1.0"));
        self.checked_at = Some(chrono::Local::now());
    }

    /// Reads what is installed (at once) and asks GitHub about every fork (in the background).
    fn refresh(&mut self, ctx: &egui::Context) {
        self.auto_updates = core::auto_updates_on();
        for (i, row) in self.rows.iter_mut().enumerate() {
            row.installed = core::installed(&row.spec);
            let spec = row.spec.clone();
            let incoming = self.incoming.clone();
            let ctx = ctx.clone();
            self.checking += 1;
            std::thread::spawn(move || {
                let state = core::fork_state(&spec);
                if let Ok(mut v) = incoming.lock() {
                    v.push((i, state));
                }
                ctx.request_repaint();
            });
        }
    }

    fn start_update(&mut self, i: usize, ctx: &egui::Context, quit_first: bool) {
        let Some(row) = self.rows.get_mut(i) else {
            return;
        };
        let Some(Ok(release)) = row.fork.as_ref().map(|f| f.release.clone()) else {
            return;
        };
        let step = Arc::new(Mutex::new(Step::Downloading {
            done: 0,
            total: None,
        }));
        row.job = Some(step.clone());
        row.relaunch = quit_first;
        let spec = row.spec.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let set = |s: Step| {
                if let Ok(mut g) = step.lock() {
                    *g = s;
                }
                ctx.request_repaint();
            };
            let result = core::install(&spec, &release, quit_first, &set);
            set(match result {
                Ok(tag) => Step::Done { tag },
                Err(e) => Step::Failed(e),
            });
        });
    }
}

const GREEN: Color32 = Color32::from_rgb(46, 160, 67);
const ORANGE: Color32 = Color32::from_rgb(214, 130, 22);
const RED: Color32 = Color32::from_rgb(207, 34, 46);

fn when(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%d.%m.%Y %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

impl eframe::App for Center {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        // Collect fork answers.
        let arrived: Vec<(usize, ForkState)> = self
            .incoming
            .lock()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default();
        for (i, state) in arrived {
            if let Some(row) = self.rows.get_mut(i) {
                row.fork = Some(state);
            }
            self.checking = self.checking.saturating_sub(1);
            if self.checking == 0 {
                self.checked_at = Some(chrono::Local::now());
            }
        }
        // Finished installs: re-read what is installed, restart what was quit.
        let mut restart_self = false;
        for row in &mut self.rows {
            let done = row
                .job
                .as_ref()
                .and_then(|j| j.lock().ok().map(|s| s.clone()))
                .filter(Step::finished);
            if let Some(Step::Done { .. }) = &done
                && row.installed.as_ref().and_then(|i| i.tag.as_ref())
                    != row
                        .fork
                        .as_ref()
                        .and_then(|f| f.release.as_ref().ok())
                        .map(|r| &r.tag)
            {
                row.installed = core::installed(&row.spec);
                if row.spec.name == SELF_NAME {
                    restart_self = true;
                } else if row.relaunch {
                    core::launch(row.spec.name, false);
                    row.relaunch = false;
                }
            }
        }

        // Which apps are open: every two seconds (each check starts a process).
        if self
            .running_checked
            .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(2))
        {
            self.running_checked = Some(std::time::Instant::now());
            for row in &mut self.rows {
                row.running = row.spec.name != SELF_NAME && core::running(row.spec.name);
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(2));

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("Craft-Zentrale");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let busy = self.checking > 0;
                    if ui
                        .add_enabled(!busy, egui::Button::new("⟳ Neu prüfen"))
                        .clicked()
                    {
                        self.refresh(ctx);
                    }
                    if busy {
                        ui.spinner();
                        ui.label("Frage GitHub …");
                    } else if let Some(t) = self.checked_at {
                        ui.label(RichText::new(format!("Geprüft um {}", t.format("%H:%M"))).weak());
                    }
                });
            });
            ui.add_space(8.0);
        });

        egui::Panel::bottom("bottom").show(ui, |ui| {
            ui.add_space(4.0);
            if self.auto_updates {
                ui.label(RichText::new("● Automatische Updates sind eingerichtet (beim Anmelden und alle 2 Stunden).").color(GREEN));
            } else {
                ui.label(RichText::new("○ Automatische Updates sind nicht eingerichtet: im Terminal  bash ~/Dev/ArtCraft-Fork/updater/install.sh").color(ORANGE));
            }
            ui.add_space(4.0);
        });

        let mut actions: Vec<(usize, bool)> = Vec::new();
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, row) in self.rows.iter().enumerate() {
                    if let Some(a) = row_ui(ui, row) {
                        actions.push((i, a));
                    }
                    ui.add_space(10.0);
                }
            });
        });
        for (i, quit_first) in actions {
            self.start_update(i, ctx, quit_first);
        }

        if restart_self {
            core::launch(SELF_NAME, true);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Draws one app; returns `Some(quit_first)` when its Update button was pressed.
fn row_ui(ui: &mut egui::Ui, row: &Row) -> Option<bool> {
    let mut action = None;
    let release: Option<&Release> = row.fork.as_ref().and_then(|f| f.release.as_ref().ok());
    let job = row
        .job
        .as_ref()
        .and_then(|j| j.lock().ok().map(|s| s.clone()));
    let busy = job.as_ref().is_some_and(|s| !s.finished());
    let state = core::compare(row.installed.as_ref(), release);

    egui::Frame::group(ui.style()).inner_margin(12.0).corner_radius(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(row.spec.name).size(18.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (text, color) = match (&row.fork, state) {
                    (None, _) => ("…", Color32::GRAY),
                    (_, Match::Same) => ("✔ Aktuell", GREEN),
                    (_, Match::UpdateAvailable) => ("Update verfügbar", ORANGE),
                    (_, Match::Unknown) => ("Version unbekannt", ORANGE),
                    (_, Match::Missing) => ("Nicht installiert", ORANGE),
                    (_, Match::NoRelease) => ("Keine Fork-Version", Color32::GRAY),
                };
                ui.label(RichText::new(text).strong().color(color));
            });
        });
        ui.add_space(6.0);

        egui::Grid::new(("grid", row.spec.repo)).num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
            // 1 – the fork on GitHub.
            ui.label(RichText::new("Fork auf GitHub").weak());
            ui.vertical(|ui| {
                match row.fork.as_ref().map(|f| &f.release) {
                    None => {
                        ui.label("wird abgefragt …");
                    }
                    Some(Ok(r)) => {
                        ui.horizontal(|ui| {
                            ui.hyperlink_to(RichText::new(&r.tag).monospace(), &r.page);
                            if let Some(p) = &r.published {
                                ui.label(RichText::new(format!("vom {}", when(p))).weak());
                            }
                        });
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(RED));
                    }
                }
                if let Some(Some(drift)) = row.fork.as_ref().map(|f| &f.drift) {
                    match drift {
                        Ok(d) if d.behind == 0 => {
                            ui.label(RichText::new("auf dem neuesten Stand des Originals").color(GREEN));
                        }
                        Ok(d) => {
                            ui.label(
                                RichText::new(format!(
                                    "Original hat {} neue Änderung{}; die nächste Sonntags-Prüfung übernimmt sie",
                                    d.behind,
                                    if d.behind == 1 { "" } else { "en" }
                                ))
                                .color(ORANGE),
                            );
                        }
                        Err(e) => {
                            ui.label(RichText::new(format!("Vergleich mit dem Original: {e}")).weak());
                        }
                    }
                }
            });
            ui.end_row();

            // 2 – what is installed.
            ui.label(RichText::new("Installiert").weak());
            ui.vertical(|ui| match &row.installed {
                None => {
                    ui.label("nicht installiert");
                }
                Some(i) => {
                    ui.horizontal(|ui| {
                        match &i.tag {
                            Some(t) => ui.label(RichText::new(t).monospace()),
                            None => ui.label(RichText::new("unbekannte Fork-Version").italics()),
                        };
                        if let Some(v) = &i.version {
                            ui.label(RichText::new(format!("Version {v}")).weak());
                        }
                    });
                    let place = i.path.parent().map(|p| p.display().to_string()).unwrap_or_default();
                    let date = i.when.map(|w| format!(" · seit {}", w.format("%d.%m.%Y %H:%M"))).unwrap_or_default();
                    ui.label(RichText::new(format!("{place}{date}")).weak());
                }
            });
            ui.end_row();

            // 3 – do they match.
            ui.label(RichText::new("Übereinstimmung").weak());
            match state {
                Match::Same => ui.label(RichText::new("✔ Installiert ist genau die neueste Fork-Version").color(GREEN)),
                Match::UpdateAvailable => ui.label(RichText::new("✖ Installiert ist eine ältere Version").color(ORANGE)),
                Match::Unknown => ui.label(RichText::new("? Von Hand installiert – Update stellt die Übereinstimmung her").color(ORANGE)),
                Match::Missing => ui.label(RichText::new("– noch nicht installiert").color(ORANGE)),
                Match::NoRelease => ui.label(RichText::new("– nichts zu vergleichen").weak()),
            };
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let wanted = matches!(state, Match::UpdateAvailable | Match::Unknown | Match::Missing);
            let can = wanted && !busy && release.is_some_and(|r| r.dmg.is_some());
            let is_self = row.spec.name == SELF_NAME;
            let running = can && !is_self && row.running;
            let label = match (state, running) {
                (Match::Missing, _) => "Installieren",
                (_, true) => "Beenden & aktualisieren",
                _ => "Update",
            };
            let button = egui::Button::new(RichText::new(label).strong()).min_size(egui::vec2(170.0, 28.0));
            let button = if can { button.fill(ui.visuals().selection.bg_fill) } else { button };
            if ui.add_enabled(can, button).on_disabled_hover_text(if busy { "Update läuft" } else { "Kein Update nötig" }).clicked() {
                action = Some(running);
            }
            if running && !busy {
                ui.label(RichText::new(format!("{} ist geöffnet und wird dafür beendet (ungesicherte Dokumente werden abgefragt).", row.spec.name)).weak());
            }
            if let Some(step) = &job {
                step_ui(ui, step, is_self);
            }
        });
    });
    action
}

/// The live display of an install.
fn step_ui(ui: &mut egui::Ui, step: &Step, is_self: bool) {
    match step {
        Step::WaitingForQuit => {
            ui.spinner();
            ui.label("Update läuft – warte, bis die App beendet ist …");
        }
        Step::Downloading { done, total } => {
            match total {
                Some(t) if *t > 0 => {
                    let f = (*done as f32 / *t as f32).clamp(0.0, 1.0);
                    ui.add(
                        egui::ProgressBar::new(f)
                            .desired_width(160.0)
                            .show_percentage(),
                    );
                    ui.label(format!(
                        "Update läuft – lade herunter ({} von {})",
                        mb(*done),
                        mb(*t)
                    ));
                }
                _ => {
                    ui.spinner();
                    ui.label(format!("Update läuft – lade herunter ({})", mb(*done)));
                }
            };
        }
        Step::Installing => {
            ui.spinner();
            ui.label("Update läuft – installiere …");
        }
        Step::Done { tag } => {
            let extra = if is_self { " – starte neu" } else { "" };
            ui.label(
                RichText::new(format!("✔ Erfolgreich durchgeführt ({tag}){extra}"))
                    .strong()
                    .color(GREEN),
            );
        }
        Step::Failed(e) => {
            ui.label(RichText::new(format!("✖ Update fehlgeschlagen: {e}")).color(RED));
        }
    }
}
