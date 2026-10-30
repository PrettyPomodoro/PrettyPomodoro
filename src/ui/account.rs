//! Account page: sign in with Google, Facebook or an email address, and manage
//! what the backend keeps — the focus history, the name on the leaderboard and
//! the account itself.

use super::widgets::{danger_link, heading, text_link, toggle_row};
use crate::core::{Core, Job, Provider, SignIn, SyncState};
use crate::theme::Theme;
use eframe::egui::{self, vec2, RichText, Ui};

#[derive(Default)]
pub struct State {
    email: String,
    password: String,
    /// The name to show on the leaderboard, while creating an account.
    name: String,
    /// The email form is creating an account rather than signing in.
    creating: bool,
    /// Editing the name of the account that is already signed in.
    rename: Option<String>,
    /// Editing the server address.
    server: Option<String>,
    /// "Delete Account" has been pressed once and is asking for confirmation.
    confirm_delete: bool,
    /// The server has been asked which sign-in buttons it offers. Without this
    /// the page would ask again on every frame until the answer arrived.
    asked_providers: bool,
    /// Scroll the profile into view, for an arrival from the header button.
    scroll_to_profile: bool,
}

impl State {
    /// Called when the user opens this page from the profile button.
    pub fn show_profile(&mut self) {
        self.scroll_to_profile = true;
    }
}

impl State {
    /// Keep the typed password no longer than it takes to send it.
    fn clear_form(&mut self) {
        self.password.clear();
        self.name.clear();
    }
}

/// Returns the background job the user asked for, if any.
pub fn show(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) -> Option<Job> {
    heading(ui, "Account", theme);
    if core.cfg.account.signed_in() {
        signed_in(ui, state, core, theme)
    } else {
        signed_out(ui, state, core, theme)
    }
}

fn dim(text: &str, theme: &Theme) -> RichText {
    RichText::new(text).size(12.5).color(theme.text_dim)
}

fn primary(label: &str, enabled: bool, theme: &Theme) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label).size(14.0).color(theme.on_accent()))
        .fill(if enabled { theme.accent } else { theme.muted })
        .min_size(vec2(240.0, 34.0))
}

fn secondary(label: &str, theme: &Theme) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label).size(14.0).color(theme.text))
        .fill(crate::theme::mix(theme.surface, theme.muted, 0.22))
        .min_size(vec2(240.0, 34.0))
}

/// A labelled single-line field.
fn field(ui: &mut Ui, label: &str, value: &mut String, password: bool, theme: &Theme) -> egui::Response {
    ui.label(dim(label, theme));
    let edit = egui::TextEdit::singleline(value)
        .password(password)
        .desired_width(f32::INFINITY)
        .margin(vec2(8.0, 6.0));
    let resp = ui.add(edit);
    ui.add_space(4.0);
    resp
}

fn signed_out(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) -> Option<Job> {
    let mut job = None;
    // Ask the server which buttons to show, once. Until it answers, all three
    // are offered; a provider it cannot honour says so when pressed.
    if core.providers.is_none() && !state.asked_providers {
        state.asked_providers = true;
        core.request(Job::Providers);
    }
    let providers = core.providers.unwrap_or_default();

    if let SignIn::InBrowser { url } = core.sign_in.clone() {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Finish signing in in your browser").size(14.5));
            ui.add_space(6.0);
            ui.label(dim("The page is open. Come back here when it says you're signed in.", theme));
            ui.add_space(10.0);
            if ui.add(secondary("Open the Page Again", theme)).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(&url));
            }
            ui.add_space(8.0);
            ui.label(dim("Waiting…", theme));
            if text_link(ui, "Cancel", 13.5, theme).clicked() {
                core.sign_in = SignIn::Idle;
                core.sign_in_attempt += 1;
            }
        });
        return None;
    }

    let working = core.sign_in == SignIn::Working;
    ui.vertical_centered(|ui| {
        ui.label(dim("Sync your history and join the leaderboard.", theme));
        ui.add_space(12.0);

        for provider in [Provider::Google, Provider::Facebook] {
            let offered = match provider {
                Provider::Google => providers.google,
                Provider::Facebook => providers.facebook,
            };
            if !offered {
                continue;
            }
            let label = format!("Continue with {}", provider.label());
            let button = match provider {
                Provider::Google => primary(&label, !working, theme),
                Provider::Facebook => secondary(&label, theme),
            };
            if ui.add_enabled(!working, button).clicked() {
                job = Some(Job::SignIn(provider));
            }
            ui.add_space(6.0);
        }

        if providers.email && (providers.google || providers.facebook) {
            ui.add_space(2.0);
            ui.label(dim("or", theme));
            ui.add_space(2.0);
        }
    });

    if providers.email {
        let submit = if state.creating { "Create Account" } else { "Sign In" };
        let creating = state.creating;
        let email = field(ui, "EMAIL", &mut state.email, false, theme);
        let password = field(ui, "PASSWORD", &mut state.password, true, theme);
        let name = creating
            .then(|| field(ui, "NAME ON THE LEADERBOARD", &mut state.name, false, theme));
        // Enter in any field submits, as a form should.
        let entered = ui.input(|i| i.key_pressed(egui::Key::Enter))
            && (email.lost_focus()
                || password.lost_focus()
                || name.is_some_and(|n| n.lost_focus()));

        ui.add_space(4.0);
        ui.vertical_centered(|ui| {
            let pressed = ui.add_enabled(!working, primary(submit, !working, theme)).clicked();
            if (pressed || entered) && !working {
                job = Some(if state.creating {
                    Job::SignUp {
                        email: state.email.clone(),
                        password: state.password.clone(),
                        display_name: state.name.clone(),
                    }
                } else {
                    Job::LogIn { email: state.email.clone(), password: state.password.clone() }
                });
                state.clear_form();
            }
            ui.add_space(2.0);
            let other = if state.creating {
                "I already have an account"
            } else {
                "Create an account instead"
            };
            if text_link(ui, other, 13.0, theme).clicked() {
                state.creating = !state.creating;
                core.sign_in = SignIn::Idle;
                state.clear_form();
            }
        });
    }

    ui.vertical_centered(|ui| {
        if working {
            ui.label(dim("Contacting the server…", theme));
        }
        if let SignIn::Failed(message) = &core.sign_in {
            ui.add_space(4.0);
            ui.label(RichText::new(message).size(12.5).color(theme.focus));
        }
    });

    ui.add_space(10.0);
    server_address(ui, state, core, theme);
    job
}

fn signed_in(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) -> Option<Job> {
    let mut job = None;
    let account = core.cfg.account.clone();
    let picture = core.avatar(account.avatar.clone().as_deref());

    if state.scroll_to_profile {
        state.scroll_to_profile = false;
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }

    // The face, and the two things it can be changed to.
    ui.vertical_centered(|ui| {
        let (face, _) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), egui::Sense::hover());
        let disc = egui::Rect::from_center_size(face.center(), vec2(76.0, 76.0));
        crate::avatar::draw(ui.painter(), disc, picture.as_ref(), &account.display_name, theme);
        let pressed = ui
            .interact(disc, ui.id().with("profile-picture"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Choose a picture")
            .clicked();

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            // Centre the pair of links under the picture.
            let width = 220.0_f32.min(ui.available_width());
            ui.add_space((ui.available_width() - width) / 2.0);
            ui.allocate_ui(vec2(width, 26.0), |ui| {
                ui.horizontal(|ui| {
                    let label =
                        if account.avatar.is_some() { "Change Picture" } else { "Add a Picture" };
                    if text_link(ui, label, 13.0, theme).clicked() || pressed {
                        job = pick_picture();
                    }
                    if account.avatar.is_some()
                        && text_link(ui, "Remove", 13.0, theme).clicked()
                    {
                        job = Some(Job::RemoveAvatar);
                    }
                });
            });
        });
        if let Some(problem) = &core.profile_error {
            ui.add(
                egui::Label::new(RichText::new(problem).size(12.0).color(theme.focus)).wrap(),
            );
        }
        ui.add_space(6.0);
    });

    ui.vertical_centered(|ui| {
        ui.label(RichText::new(format!("Signed in as {}", account.display_name)).size(15.0));
        let how = match (account.provider.as_deref(), &account.email) {
            (Some("google"), _) => "through Google".to_owned(),
            (Some("facebook"), _) => "through Facebook".to_owned(),
            (_, Some(email)) => email.clone(),
            _ => String::new(),
        };
        if !how.is_empty() {
            ui.label(dim(&how, theme));
        }
        ui.add_space(10.0);

        let running = core.sync_state == SyncState::Running;
        let label = if running { "Syncing…" } else { "Sync Now" };
        if ui.add_enabled(!running, primary(label, !running, theme)).clicked() {
            job = Some(Job::Sync);
        }
        ui.add_space(4.0);
        match &core.sync_state {
            SyncState::Idle | SyncState::Running => {}
            SyncState::Done(message) => {
                ui.label(RichText::new(message).size(12.5).color(theme.short_break));
            }
            SyncState::Failed(message) => {
                ui.label(RichText::new(message).size(12.5).color(theme.focus));
            }
        }
        if let Some(when) = &account.last_sync {
            ui.label(dim(&format!("Last synced {when}"), theme));
        }
        ui.label(dim("Syncs on launch and after every focus round.", theme));
    });

    // The name on the leaderboard, editable in place.
    ui.add_space(14.0);
    match state.rename.clone() {
        None => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Leaderboard Name").size(14.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if text_link(ui, "Edit", 13.0, theme).clicked() {
                        state.rename = Some(account.display_name.clone());
                    }
                });
            });
            ui.label(dim(&account.display_name, theme));
        }
        Some(mut draft) => {
            let resp = field(ui, "LEADERBOARD NAME", &mut draft, false, theme);
            let entered = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            state.rename = Some(draft.clone());
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 8.0) / 2.0;
                let save = ui.add_sized([w, 30.0], egui::Button::new("Save")).clicked();
                if save || entered {
                    core.cfg.account.display_name = draft.trim().to_owned();
                    state.rename = None;
                    job = Some(Job::SaveProfile);
                }
                if ui.add_sized([w, 30.0], egui::Button::new("Cancel")).clicked() {
                    state.rename = None;
                }
            });
        }
    }

    ui.add_space(10.0);
    if toggle_row(ui, "Show Me on the Leaderboard", &mut core.cfg.account.show_on_leaderboard, theme) {
        job = Some(Job::SaveProfile);
    }
    ui.label(dim(
        "Your name and today's focus minutes appear on the board. Switch this off \
         to keep your minutes to yourself; your history still syncs.",
        theme,
    ));

    // Goal texts are personal, so syncing them is something to opt into.
    ui.add_space(10.0);
    if toggle_row(ui, "Sync Goal Texts", &mut core.cfg.account.sync_goals, theme)
        && core.cfg.account.sync_goals
    {
        job = Some(Job::Sync);
    }
    ui.label(dim(
        "Also keeps the goals you tick off on the server, so your goal history \
         follows you to your other computers. Goal texts never appear on the board.",
        theme,
    ));

    ui.add_space(14.0);
    ui.vertical_centered(|ui| {
        if text_link(ui, "Sign Out", 13.5, theme).clicked() {
            state.confirm_delete = false;
            job = Some(Job::SignOut);
        }
        ui.add_space(2.0);
        if state.confirm_delete {
            ui.label(dim("This erases your account and its history on the server.", theme));
            if danger_link(ui, "Delete Everything", 13.5, theme).clicked() {
                state.confirm_delete = false;
                job = Some(Job::DeleteAccount);
            }
            if text_link(ui, "Keep My Account", 13.0, theme).clicked() {
                state.confirm_delete = false;
            }
        } else if text_link(ui, "Delete Account", 13.0, theme).clicked() {
            state.confirm_delete = true;
        }
    });

    ui.add_space(10.0);
    server_address(ui, state, core, theme);
    job
}

/// Ask the user for an image file. The native dialog has to run on this, the
/// main thread; it blocks the window while it is open, and the file is read and
/// shrunk in the background afterwards.
fn pick_picture() -> Option<Job> {
    let file = rfd::FileDialog::new()
        .set_title("Choose a profile picture")
        .add_filter("Images", &crate::avatar::EXTENSIONS)
        .pick_file()?;
    Some(Job::UploadAvatar(file))
}

/// Where the backend lives. Tucked away, since almost nobody needs to change it.
fn server_address(ui: &mut Ui, state: &mut State, core: &mut Core, theme: &Theme) {
    let signed_in = core.cfg.account.signed_in();
    match state.server.clone() {
        None => {
            ui.vertical_centered(|ui| {
                let label = format!("Server: {}", pretty_server(&core.cfg.account.server));
                // Changing it mid-session would leave a token from another server.
                if signed_in {
                    ui.label(dim(&label, theme))
                        .on_hover_text("Sign out to point the app at another server.");
                } else if text_link(ui, &label, 11.5, theme).clicked() {
                    state.server = Some(core.cfg.account.server.clone());
                }
            });
        }
        Some(mut draft) => {
            ui.label(dim("SERVER ADDRESS", theme));
            let edit = egui::TextEdit::singleline(&mut draft)
                .hint_text(crate::api::DEFAULT_SERVER)
                .desired_width(f32::INFINITY)
                .margin(vec2(8.0, 6.0));
            ui.add(edit);
            state.server = Some(draft.clone());
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 8.0) / 2.0;
                if ui.add_sized([w, 28.0], egui::Button::new("Use This")).clicked() {
                    core.cfg.account.server = draft.trim().to_owned();
                    // A different server has its own accounts, board and buttons.
                    core.providers = None;
                    core.board = Default::default();
                    core.sign_in = SignIn::Idle;
                    state.asked_providers = false;
                    state.server = None;
                }
                if ui.add_sized([w, 28.0], egui::Button::new("Cancel")).clicked() {
                    state.server = None;
                }
            });
            if ui.add_sized([ui.available_width(), 24.0], egui::Button::new("Reset to Default")).clicked() {
                state.server = Some(crate::api::DEFAULT_SERVER.to_owned());
            }
        }
    }
}

/// The server address without its scheme, which is noise on a narrow page.
fn pretty_server(server: &str) -> &str {
    server
        .trim()
        .trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_is_shown_without_its_scheme() {
        assert_eq!(pretty_server("https://api.example.com/"), "api.example.com");
        assert_eq!(pretty_server("http://127.0.0.1:8787"), "127.0.0.1:8787");
        assert_eq!(pretty_server(""), "");
    }

    #[test]
    fn submitting_the_form_does_not_leave_the_password_behind() {
        let mut state = State { password: "secret".into(), name: "Ada".into(), ..Default::default() };
        state.clear_form();
        assert!(state.password.is_empty() && state.name.is_empty());
    }
}
