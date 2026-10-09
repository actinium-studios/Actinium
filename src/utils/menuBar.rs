use std::sync::mpsc::Receiver;

use crate::utils::update_checker::{check_for_update_async, UpdateNotice};

trait MenuUiExt {
    fn close_menu(&mut self);
}

impl MenuUiExt for egui::Ui {
    fn close_menu(&mut self) {
        self.close();
    }
}

pub struct MenuBarState {
    pub show_source_control: bool,
    pub show_repo_picker: bool,
    pub show_about: bool,
    logo_texture: Option<egui::TextureHandle>,
    update_rx: Option<Receiver<UpdateNotice>>,
    update_notice: Option<UpdateNotice>,
}

impl MenuBarState {
    pub fn new() -> Self {
        Self {
            show_source_control: false,
            show_repo_picker: false,
            show_about: false,
            logo_texture: None,
            update_rx: None,
            update_notice: None,
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if let Some(rx) = &self.update_rx {
            if let Ok(notice) = rx.try_recv() {
                self.update_notice = Some(notice);
                self.update_rx = None;
            }
        }

        if self.logo_texture.is_none() {
            let image = image::load_from_memory(include_bytes!("../../assets/logo128.png"))
                .expect("failed to load About logo")
                .into_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            self.logo_texture = Some(ui.ctx().load_texture(
                "actinium-about-logo",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }

        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project").clicked() {
                        // TODO: wire to your project-creation flow
                        ui.close_menu();
                    }
                    if ui.button("Open Project...").clicked() {
                        // TODO: file dialog (e.g. via `rfd` crate)
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Save").clicked() {
                        // TODO: save current project
                        ui.close_menu();
                    }
                    if ui.button("Save As...").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Redo").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Cut").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Copy").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Paste").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Preferences...").clicked() {
                        ui.close_menu();
                    }
                });

                ui.menu_button("Tools", |ui| {
                    if ui.button("Asset Importer").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Build Settings").clicked() {
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("Scene Hierarchy").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Inspector").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Console").clicked() {
                        ui.close_menu();
                    }
                });

                ui.menu_button("Utils", |ui| {
                    if ui.checkbox(&mut self.show_source_control, "Source Control").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_repo_picker, "Clone Repository...").clicked() {
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Documentation").clicked() {
                        let _ = open::that("https://docs.actinium.top/Actinium/gettingStarted.html");
                        ui.close_menu();
                    }
                    if ui.button("Report an Issue").clicked() {
                        let _ = open::that("https://github.com/actinium-studios/Actinium/issues");
                        ui.close_menu();
                    }
                    if ui.button("Report a Security Vulnerability").clicked() {
                        let _ = open::that("https://github.com/actinium-studios/Actinium/security/advisories/new");
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("About Actinium").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });

        if self.show_about {
            let logo_texture = self
                .logo_texture
                .as_ref()
                .expect("About logo texture was not initialized")
                .clone();
            egui::Window::new("About Actinium")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.image((logo_texture.id(), egui::vec2(128.0, 128.0)));
                        ui.heading("Actinium Game Engine");
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        ui.label(format!(
                            "Build channel: {}",
                            crate::utils::update_checker::build_channel()
                        ));
                        ui.label("Developed with love by Syrup Studios.");
                        if crate::utils::update_checker::build_channel() == "stable" {
                            if ui.button("Check for Updates").clicked() {
                                self.update_notice = None;
                                self.update_rx =
                                    Some(check_for_update_async(env!("CARGO_PKG_VERSION")));
                            }

                            if let Some(notice) = &self.update_notice {
                                match notice {
                                    UpdateNotice::None => {
                                        ui.label("You are up to date.");
                                    }
                                    UpdateNotice::Available { version, url } => {
                                        ui.label(format!("Update available: {version}"));
                                        if ui.link("View release").clicked() {
                                            let _ = open::that(url);
                                        }
                                    }
                                    UpdateNotice::CheckFailed(error) => {
                                        ui.label(format!("Update check failed: {error}"));
                                    }
                                }
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Close").clicked() {
                        self.show_about = false;
                    }
                });
        }
    }
}
