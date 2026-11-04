use egui::{Align, Color32, Frame, Layout, Margin, RichText, Stroke, Ui, Vec2};

use crate::app::App;
use crate::equalizer::{
    BAND_COUNT, BAND_LABELS, MAX_GAIN, MAX_PREAMP, MIN_GAIN, MIN_PREAMP, PRESETS,
};
use crate::settings::ThemeVariant;
use crate::ui::theme::{accent, palette, Palette, ACCENT_PRESETS};

pub fn settings(app: &mut App, ui: &mut Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .show(ui, |ui| {
            ui.add_space(10.0);
            ui.label(
                RichText::new("Settings")
                    .size(28.0)
                    .color(pal.text)
                    .strong(),
            );
            ui.add_space(14.0);

            section(ui, &pal, "Appearance");
            card(ui, &pal, |ui| {
                ui.label(RichText::new("Accent color").size(16.0).color(pal.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Applies to the seek bar, active icons, selection and sliders.")
                        .size(13.0)
                        .color(pal.muted),
                );
                ui.add_space(12.0);
                if swatches(app, ui, &pal) {
                    app.reload_theme();
                    app.persist(true);
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Hex").size(13.0).color(pal.muted));
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut app.accent_hex)
                            .desired_width(120.0)
                            .font(egui::TextStyle::Body),
                    );
                    let submitted =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let apply = ui
                        .add(
                            egui::Button::new(RichText::new("Apply").color(pal.text))
                                .min_size(Vec2::new(96.0, 44.0)),
                        )
                        .clicked();
                    if apply || submitted {
                        app.apply_hex_accent();
                    }
                });
            });

            ui.add_space(12.0);

            card(ui, &pal, |ui| {
                ui.label(RichText::new("Theme").size(16.0).color(pal.text));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    for (variant, label) in [
                        (ThemeVariant::Dark, "Dark"),
                        (ThemeVariant::Amoled, "AMOLED black"),
                    ] {
                        let selected = app.settings.theme == variant;
                        if crate::ui::widgets::pill_button(ui, label, selected, accent, &pal) {
                            app.settings.theme = variant;
                            app.reload_theme();
                            app.persist(true);
                        }
                        ui.add_space(4.0);
                    }
                });
            });

            ui.add_space(12.0);

            section(ui, &pal, "Offline");
            card(ui, &pal, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Offline mode").size(16.0).color(pal.text));
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!(
                                "Play only downloaded and local tracks. {}",
                                if !app.online {
                                    "No internet connection detected."
                                } else if app.settings.offline_mode {
                                    "Forced on while you are connected."
                                } else {
                                    "Turns on automatically when the connection drops."
                                }
                            ))
                            .size(13.0)
                            .color(pal.muted),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if crate::ui::widgets::toggle_switch(
                            ui,
                            app.settings.offline_mode,
                            accent,
                            &pal,
                        ) {
                            let on = !app.settings.offline_mode;
                            app.set_offline_mode(on);
                        }
                    });
                });
            });

            ui.add_space(12.0);

            #[cfg(not(target_arch = "wasm32"))]
            {
            section(ui, &pal, "Import");
            card(ui, &pal, |ui| {
                ui.label(RichText::new("JioSaavn source").size(16.0).color(pal.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Leave empty to use the direct JioSaavn API with a proxy fallback. \
                         Set a base URL to route through a mirror, or the ARIA_JIOSAAVN_BASE \
                         environment variable to override it without saving.",
                    )
                    .size(13.0)
                    .color(pal.muted),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut app.settings.jiosaavn_base)
                            .hint_text("https://www.jiosaavn.com/api.php")
                            .desired_width(360.0)
                            .font(egui::TextStyle::Body),
                    );
                    if response.changed() {
                        #[cfg(not(target_arch = "wasm32"))]
                        crate::provider_jiosaavn::set_base(&app.settings.jiosaavn_base);
                        app.touch_settings();
                    }
                    if crate::ui::widgets::pill_button(ui, "Reset", false, accent, &pal) {
                        app.settings.jiosaavn_base.clear();
                        #[cfg(not(target_arch = "wasm32"))]
                        crate::provider_jiosaavn::set_base("");
                        app.touch_settings();
                    }
                });
            });

            ui.add_space(12.0);

            }

            equalizer_card(app, ui, accent, &pal);

            ui.add_space(12.0);

            section(ui, &pal, "Audio and EQ");
            card(ui, &pal, |ui| {
                ui.label(RichText::new("Playback").size(16.0).color(pal.text));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Volume").size(13.5).color(pal.muted));
                    let mut volume = app.settings.volume;
                    if ui
                        .add(
                            egui::Slider::new(&mut volume, 0.0..=1.0)
                                .show_value(false)
                                .trailing_fill(true),
                        )
                        .changed()
                    {
                        app.set_volume(volume);
                        app.persist(true);
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Autoplay a radio mix when the queue ends")
                            .size(13.5)
                            .color(pal.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if crate::ui::widgets::toggle_switch(
                            ui,
                            app.settings.autoplay,
                            accent,
                            &pal,
                        ) {
                            app.settings.autoplay = !app.settings.autoplay;
                            app.touch_settings();
                        }
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Volume leveling (ReplayGain-style)")
                            .size(13.5)
                            .color(pal.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if crate::ui::widgets::toggle_switch(
                            ui,
                            app.settings.normalize,
                            accent,
                            &pal,
                        ) {
                            app.settings.normalize = !app.settings.normalize;
                            app.set_normalize(app.settings.normalize);
                            app.touch_settings();
                        }
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Crossfade").size(13.5).color(pal.muted));
                    let mut crossfade = app.settings.crossfade;
                    if ui
                        .add(
                            egui::Slider::new(&mut crossfade, 0.0..=12.0)
                                .suffix(" s")
                                .fixed_decimals(1),
                        )
                        .changed()
                    {
                        app.settings.crossfade = crossfade;
                        app.touch_settings();
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Battery saver").size(13.5).color(pal.muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if crate::ui::widgets::toggle_switch(
                            ui,
                            app.settings.battery_saver,
                            accent,
                            &pal,
                        ) {
                            app.settings.battery_saver = !app.settings.battery_saver;
                            app.persist(true);
                        }
                    });
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Slows repaints and pauses new downloads to save power.",
                    )
                    .size(12.0)
                    .color(pal.muted),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Sleep timer").size(13.5).color(pal.muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(label) = app.sleep.label() {
                            if crate::ui::widgets::pill_button(ui, "Cancel", false, accent, &pal) {
                                app.cancel_sleep();
                            }
                            ui.label(RichText::new(label).size(12.0).color(accent));
                        } else {
                            if crate::ui::widgets::pill_button(ui, "End of track", false, accent, &pal) {
                                app.arm_sleep_end_of_track();
                            }
                            for minutes in [60u32, 30, 15] {
                                if crate::ui::widgets::pill_button(
                                    ui,
                                    &format!("{minutes} min"),
                                    false,
                                    accent,
                                    &pal,
                                ) {
                                    app.arm_sleep_minutes(minutes);
                                }
                            }
                        }
                    });
                });
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("Config: {}", crate::settings::data_dir().display()))
                            .size(12.5)
                            .color(pal.muted),
                    );
                }
            });

            ui.add_space(12.0);

            #[cfg(not(target_arch = "wasm32"))]
            {
            section(ui, &pal, "Miniplayer");
            card(ui, &pal, |ui| {
                ui.label(RichText::new("Miniplayer").size(16.0).color(pal.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Borderless always-on-top window. Open it from the mini button or Ctrl+M.",
                    )
                    .size(13.0)
                    .color(pal.muted),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Opacity").size(13.5).color(pal.muted));
                    let mut opacity = app.settings.mini_opacity;
                    let changed = ui
                        .add(
                            egui::Slider::new(&mut opacity, crate::miniplayer::MIN_OPACITY..=1.0)
                                .show_value(false)
                                .trailing_fill(true),
                        )
                        .changed();
                    ui.label(
                        RichText::new(format!(
                            "{}%",
                            (app.settings.mini_opacity * 100.0).round() as i32
                        ))
                        .size(12.5)
                        .color(pal.muted),
                    );
                    if changed {
                        app.settings.mini_opacity = opacity;
                        app.mini.opacity = opacity;
                        app.persist(true);
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Size").size(13.5).color(pal.muted));
                    for (label, width, height) in crate::miniplayer::PRESETS {
                        if crate::ui::widgets::pill_button(ui, label, false, accent, &pal) {
                            app.settings.mini.width = width;
                            app.settings.mini.height = height;
                            ui.ctx().send_viewport_cmd_to(
                                crate::miniplayer::viewport_id(),
                                egui::ViewportCommand::InnerSize(egui::vec2(width, height)),
                            );
                            app.persist(true);
                        }
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Auto-fade when idle")
                            .size(13.5)
                            .color(pal.muted),
                    );
                    for (on, label) in [(false, "Off"), (true, "On")] {
                        if crate::ui::widgets::pill_button(
                            ui,
                            label,
                            app.settings.mini_auto_fade == on,
                            accent,
                            &pal,
                        ) {
                            app.settings.mini_auto_fade = on;
                            app.persist(true);
                        }
                        ui.add_space(4.0);
                    }
                });
            });

            ui.add_space(12.0);

            }

            #[cfg(not(target_arch = "wasm32"))]
            {
            section(ui, &pal, "Downloads");
            card(ui, &pal, |ui| {
                ui.label(RichText::new("Download storage").size(16.0).color(pal.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(crate::downloads::dir().display().to_string())
                        .size(12.5)
                        .color(pal.muted),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if crate::ui::widgets::accent_button(
                        ui,
                        "Open folder",
                        egui::vec2(140.0, 44.0),
                        accent,
                    ) {
                        let folder = crate::downloads::dir();
                        crate::proc::open_folder(&folder);
                    }
                    ui.add_space(8.0);
                    if crate::ui::widgets::pill_button(ui, "Rescan folders", false, accent, &pal) {
                        app.rescan_folders();
                    }
                });
                ui.add_space(10.0);
                if crate::ui::widgets::pill_button(ui, "Delete all downloads…", false, accent, &pal) {
                    let (count, bytes) = crate::downloads::stats();
                    if count == 0 {
                        app.notify("There are no downloads to delete");
                    } else {
                        app.delete_all = Some((1, count, bytes));
                    }
                }
            });
            delete_all_dialog(app, ui.ctx(), &pal);

            ui.add_space(12.0);

            }

            #[cfg(target_arch = "wasm32")]
            {
                section(ui, &pal, "Install");
                card(ui, &pal, |ui| {
                    let state = crate::install::state();
                    let (headline, hint) = match state.as_str() {
                        "installed" => ("Aria is installed on this device", "You are using the installed app."),
                        "available" => ("Install Aria as an app", "Adds Aria to your home screen or desktop and opens it in its own window, even offline."),
                        "ios" => ("Add Aria to your home screen", "Tap the Share button in Safari, then Add to Home Screen."),
                        _ => ("Install Aria as an app", "Use your browser menu: Install app, or Add to Home screen."),
                    };
                    ui.label(RichText::new(headline).size(16.0).color(pal.text));
                    ui.add_space(4.0);
                    ui.label(RichText::new(hint).size(12.5).color(pal.muted));
                    if state == "available" {
                        ui.add_space(10.0);
                        if crate::ui::widgets::pill_button(ui, "Install", true, accent, &pal) {
                            crate::install::install();
                        }
                    }
                });
                ui.add_space(12.0);
            }

            #[cfg(not(target_arch = "wasm32"))]
            {
            section(ui, &pal, "Updates");
            card(ui, &pal, |ui| {
                use crate::update::State;
                let version = env!("CARGO_PKG_VERSION");
                let status = match &app.update {
                    State::Idle => format!("Aria {version}"),
                    State::Checking => "Checking for updates...".to_string(),
                    State::UpToDate => format!("Aria {version} is the latest version"),
                    State::Available(release) => format!("Version {} is available (you have {version})", release.version),
                    State::Downloading(release) => format!("Downloading version {}...", release.version),
                    State::Ready(_) => "Update downloaded and ready to install".to_string(),
                    State::Failed(reason) => format!("Update problem: {reason}"),
                };
                ui.label(RichText::new(status).size(14.0).color(pal.text));
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    let busy = matches!(app.update, State::Checking | State::Downloading(_));
                    if crate::ui::widgets::pill_button(ui, "Check for updates", false, accent, &pal) && !busy {
                        app.check_for_updates();
                    }
                    match app.update.clone() {
                        State::Available(release) => {
                            let label = if release.asset.is_some() { "Download and install" } else { "Open download page" };
                            if crate::ui::widgets::pill_button(ui, label, true, accent, &pal) {
                                app.install_update();
                            }
                        }
                        State::Ready(_) if crate::ui::widgets::pill_button(ui, "Install now", true, accent, &pal) => {
                            let ctx = ui.ctx().clone();
                            app.launch_update(&ctx);
                        }
                        _ => {}
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Aria also checks when it starts. New versions are published on the GitHub releases page.").size(12.5).color(pal.muted));
            });
            ui.add_space(12.0);

            }

            section(ui, &pal, "About");
            card(ui, &pal, |ui| {
                ui.label(RichText::new("About").size(16.0).color(pal.text));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Aria {} · memory {}", env!("CARGO_PKG_VERSION"), crate::stats::rss_label()))
                        .size(13.0)
                        .color(pal.muted),
                );
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("Library: {}", crate::settings::data_dir().display()))
                            .size(12.5)
                            .color(pal.muted),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!("Downloads: {}", crate::downloads::dir().display()))
                            .size(12.5)
                            .color(pal.muted),
                    );
                }
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Errors are written to aria.log in the library folder")
                        .size(12.5)
                        .color(pal.muted),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Run with --stats to print live memory usage")
                        .size(12.5)
                        .color(pal.muted),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if crate::ui::widgets::accent_button(
                        ui,
                        "Keyboard shortcuts",
                        egui::vec2(190.0, 44.0),
                        accent,
                    ) {
                        app.help_open = true;
                    }
                    ui.add_space(8.0);
                    if crate::ui::widgets::pill_button(ui, "Update yt-dlp", false, accent, &pal) {
                        let _ = app.jobs.tx.send(crate::jobs::Job::DownloadYtDlp);
                        app.notify("Updating yt-dlp...");
                    }
                });
                ui.add_space(6.0);
                if let Some((message, ready)) = app.ytdlp_status.clone() {
                    ui.label(
                        RichText::new(format!(
                            "{message}{}",
                            if ready { "" } else { " - press Update yt-dlp" }
                        ))
                        .size(12.0)
                        .color(pal.muted),
                    );
                }
            });

            ui.add_space(12.0);

            section(ui, &pal, "Region suggestions");
            card(ui, &pal, |ui| {
                ui.label(RichText::new("Trending songs for your region").size(16.0).color(pal.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Shows trending songs on Home. \"My location\" looks up your approximate state from your IP address through ipwho.is each time you press it; nothing is stored except the state and language. Choosing a language yourself sends no location at all.",
                    )
                    .size(12.5)
                    .color(pal.muted),
                );
                ui.add_space(10.0);
                let mode = app.settings.region_mode;
                ui.horizontal_wrapped(|ui| {
                    if crate::ui::widgets::pill_button(ui, "Off", mode == crate::settings::RegionMode::Off, accent, &pal) {
                        app.set_region_mode(crate::settings::RegionMode::Off);
                    }
                    if crate::ui::widgets::pill_button(ui, "My location", mode == crate::settings::RegionMode::Auto, accent, &pal) {
                        app.set_region_mode(crate::settings::RegionMode::Auto);
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Or pick a language").size(12.5).color(pal.muted));
                let current = app.settings.region_language.clone();
                ui.horizontal_wrapped(|ui| {
                    for (id, label) in crate::region::LANGUAGES {
                        let active = mode == crate::settings::RegionMode::Manual && current.as_deref() == Some(id);
                        if crate::ui::widgets::pill_button(ui, label, active, accent, &pal) {
                            app.choose_region_language(id);
                        }
                    }
                });
                if mode == crate::settings::RegionMode::Auto {
                    if let (Some(language), label) = (current, app.settings.region_label.clone()) {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "Detected {} - showing {} songs",
                                label.unwrap_or_else(|| "your region".to_string()),
                                crate::region::language_label(&language)
                            ))
                            .size(12.5)
                            .color(pal.muted),
                        );
                    }
                }
            });

            ui.add_space(12.0);

            #[cfg(not(target_arch = "wasm32"))]
            {
            section(ui, &pal, "Agent mode");
            card(ui, &pal, |ui| {
                ui.label(RichText::new("Let an agent drive Aria").size(16.0).color(pal.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Off by default. When on, a program on this computer that has the secret token can fully control the player (play, search, open pages, resize the window, take screenshots, read the error log) and ask Aria to check itself for problems. It listens on this computer only and never on the network.",
                    )
                    .size(12.5)
                    .color(pal.muted),
                );
                ui.add_space(10.0);
                let enabled = app.settings.agent_enabled;
                ui.horizontal_wrapped(|ui| {
                    if crate::ui::widgets::pill_button(ui, "Off", !enabled, accent, &pal) && enabled {
                        app.settings.agent_enabled = false;
                        app.sync_agent();
                        app.touch_settings();
                    }
                    if crate::ui::widgets::pill_button(ui, "On", enabled, accent, &pal) && !enabled {
                        app.settings.agent_enabled = true;
                        app.sync_agent();
                        app.touch_settings();
                    }
                    if crate::ui::widgets::pill_button(ui, "Run diagnostics", false, accent, &pal) {
                        app.run_doctor();
                    }
                });
                if let Some(handle) = app.agent.as_ref() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "Listening on 127.0.0.1:{}. Token file: {}",
                            handle.port,
                            handle.token_path.display()
                        ))
                        .size(12.0)
                        .color(pal.muted),
                    );
                }
                if let Some(error) = app.agent_error.clone() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(error).size(12.0).color(egui::Color32::from_rgb(220, 68, 92)));
                }
                for finding in app.agent_findings.clone() {
                    ui.add_space(8.0);
                    let color = match finding.severity {
                        crate::doctor::Severity::Error => egui::Color32::from_rgb(220, 68, 92),
                        crate::doctor::Severity::Warn => egui::Color32::from_rgb(230, 170, 60),
                        crate::doctor::Severity::Info => pal.muted,
                    };
                    ui.label(RichText::new(format!("{}: {}", finding.severity.label(), finding.title)).size(13.0).color(color).strong());
                    if !finding.detail.is_empty() {
                        ui.label(RichText::new(finding.detail).size(12.0).color(pal.muted));
                    }
                    if !finding.suggestion.is_empty() {
                        ui.label(RichText::new(format!("Fix: {}", finding.suggestion)).size(12.0).color(pal.text));
                    }
                }
            });

            ui.add_space(12.0);

            }

            #[cfg(not(target_arch = "wasm32"))]
            {
            section(ui, &pal, "YouTube sign-in");
            card(ui, &pal, |ui| {
                ui.label(RichText::new("Use my browser's YouTube login").size(16.0).color(pal.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Off by default. Lets yt-dlp read the login saved in the browser you pick, so Mixes match your browser and \"sign in\" blocks go away. yt-dlp can then act as that signed-in account, and automated use can get an account flagged, so a separate account is safest. Firefox works best; recent Chrome, Edge and Brave on Windows often cannot be read.",
                    )
                    .size(12.5)
                    .color(pal.muted),
                );
                ui.add_space(10.0);
                let current = app.settings.ytdlp_cookie_browser.clone();
                let mut chosen = current.clone();
                ui.horizontal_wrapped(|ui| {
                    if crate::ui::widgets::pill_button(ui, "Off", current.is_none(), accent, &pal) {
                        chosen = None;
                    }
                    for (id, label) in crate::ytdlp::browsers_here() {
                        if crate::ui::widgets::pill_button(ui, label, current.as_deref() == Some(id), accent, &pal) {
                            chosen = Some(id.to_string());
                        }
                    }
                });
                if chosen != current {
                    crate::ytdlp::set_cookie_browser(chosen.as_deref());
                    app.settings.ytdlp_cookie_browser = chosen;
                    app.persist(true);
                    app.touch_settings();
                }
                if let Some(error) = crate::ytdlp::cookie_error() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("Could not read that browser's login, continuing signed out: {error}"))
                            .size(12.0)
                            .color(egui::Color32::from_rgb(220, 68, 92)),
                    );
                }
            });

            }

            ui.add_space(24.0);
        });
}

fn equalizer_card(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    card(ui, pal, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Equalizer").size(16.0).color(pal.text));
            ui.add_space(8.0);
            ui.label(
                RichText::new(app.settings.eq.preset.clone())
                    .size(12.5)
                    .color(pal.muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if crate::ui::widgets::toggle_switch(
                    ui,
                    app.settings.eq.enabled,
                    accent,
                    pal,
                ) {
                    app.settings.eq.enabled = !app.settings.eq.enabled;
                    app.apply_eq();
                }
            });
        });
        ui.label(
            RichText::new(
                "10-band graphic EQ applied live to local and streamed audio. Adjusting a band switches the preset to Custom.",
            )
            .size(13.0)
            .color(pal.muted),
        );
        ui.add_space(12.0);

        ui.horizontal_wrapped(|ui| {
            for preset in PRESETS.iter() {
                let selected = app.settings.eq.matching_preset() == Some(preset.name);
                if crate::ui::widgets::pill_button(ui, preset.name, selected, accent, pal) {
                    app.settings.eq.apply_preset(preset.name);
                    app.apply_eq();
                }
                ui.add_space(4.0);
            }
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            let custom = app.settings.eq.custom.clone();
            for preset in &custom {
                let selected = app.settings.eq.preset.eq_ignore_ascii_case(&preset.name);
                if crate::ui::widgets::pill_button(ui, &preset.name, selected, accent, pal) {
                    app.apply_eq_custom(&preset.name);
                }
                if crate::ui::widgets::pill_button(ui, "x", false, accent, pal) {
                    app.delete_eq_preset(&preset.name);
                }
                ui.add_space(8.0);
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.eq_preset_name)
                    .hint_text("Name this curve")
                    .desired_width(200.0),
            );
            if crate::ui::widgets::pill_button(ui, "Save preset", false, accent, pal) {
                app.save_eq_preset();
            }
        });
        ui.add_space(16.0);

        let band_tint = if app.settings.eq.enabled {
            accent
        } else {
            accent.gamma_multiply(0.45)
        };
        ui.horizontal(|ui| {
            let total = BAND_COUNT as f32 * 52.0;
            ui.add_space(((ui.available_width() - total) * 0.5).max(6.0));
            for (band, label) in BAND_LABELS.iter().enumerate() {
                ui.allocate_ui_with_layout(
                    Vec2::new(52.0, 168.0),
                    Layout::top_down(Align::Center),
                    |ui| {
                        let gain = app.settings.eq.gains[band];
                        ui.label(
                            RichText::new(format!("{gain:+.0}"))
                                .size(10.5)
                                .color(if gain.abs() > 0.01 { band_tint } else { pal.muted }),
                        );
                        if let Some(value) = crate::ui::widgets::eq_slider(
                            ui, gain, MIN_GAIN, MAX_GAIN, band_tint, pal,
                        ) {
                            app.settings.eq.set_gain(band, value);
                            app.apply_eq();
                        }
                        ui.label(
                            RichText::new(*label)
                                .size(11.5)
                                .color(pal.muted),
                        );
                    },
                );
            }
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Preamp").size(13.5).color(pal.muted));
            let mut preamp = app.settings.eq.preamp;
            let slider = ui.add(
                egui::Slider::new(&mut preamp, MIN_PREAMP..=MAX_PREAMP)
                    .suffix(" dB")
                    .fixed_decimals(0),
            );
            if slider.changed() {
                app.settings.eq.preamp = preamp;
                app.apply_eq();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_sized(
                        [96.0, 44.0],
                        egui::Button::new(RichText::new("Reset").size(14.0).color(pal.text)),
                    )
                    .clicked()
                {
                    app.settings.eq.gains = [0.0; BAND_COUNT];
                    app.settings.eq.preamp = 0.0;
                    app.settings.eq.preset = "Flat".to_string();
                    app.apply_eq();
                }
            });
        });
    });
}

fn section(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(12.0)
            .color(pal.muted)
            .strong(),
    );
    ui.add_space(6.0);
}

fn card(ui: &mut Ui, pal: &Palette, contents: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(pal.surface)
        .stroke(Stroke::new(1.0, pal.line))
        .corner_radius(egui::CornerRadius::same(16))
        .inner_margin(Margin::symmetric(16, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
}

fn swatches(app: &mut App, ui: &mut Ui, pal: &Palette) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for preset in ACCENT_PRESETS {
            let color = Color32::from_rgb(preset[0], preset[1], preset[2]);
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(44.0), egui::Sense::click());
            ui.painter().circle_filled(rect.center(), 18.0, color);
            if app.settings.accent == preset {
                ui.painter()
                    .circle_stroke(rect.center(), 21.0, Stroke::new(2.0, pal.text));
            } else if response.hovered() {
                ui.painter()
                    .circle_stroke(rect.center(), 21.0, Stroke::new(1.5, pal.muted));
            }
            if response.clicked() {
                app.settings.accent = preset;
                app.accent_hex = crate::ui::theme::accent_hex(&app.settings);
                changed = true;
            }
            ui.add_space(4.0);
        }
    });
    changed
}

fn delete_all_dialog(app: &mut App, ctx: &egui::Context, pal: &Palette) {
    let Some((stage, count, bytes)) = app.delete_all else {
        return;
    };
    let size = format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0));
    let noun = if count == 1 { "file" } else { "files" };
    let mut cancel = false;
    let mut next = false;
    let mut confirm = false;
    egui::Window::new("Delete all downloads")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            if stage == 1 {
                ui.label(
                    RichText::new(format!("{count} {noun}, {size} will be removed from disk."))
                        .color(pal.text),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    next = crate::ui::widgets::pill_button(ui, "Continue", false, pal.text, pal);
                    cancel = crate::ui::widgets::pill_button(ui, "Cancel", false, pal.muted, pal);
                });
            } else {
                ui.label(
                    RichText::new(format!(
                        "This cannot be undone. Delete {count} {noun} ({size}) permanently?"
                    ))
                    .color(pal.text),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    confirm = crate::ui::widgets::pill_button(
                        ui,
                        "Yes, delete everything",
                        true,
                        egui::Color32::from_rgb(220, 68, 92),
                        pal,
                    );
                    cancel = crate::ui::widgets::pill_button(ui, "Cancel", false, pal.muted, pal);
                });
            }
        });
    if cancel {
        app.delete_all = None;
    } else if next {
        app.delete_all = Some((2, count, bytes));
    } else if confirm {
        app.clear_downloads();
    }
}
