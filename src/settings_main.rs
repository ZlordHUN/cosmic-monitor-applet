// SPDX-License-Identifier: MPL-2.0

mod config;
mod i18n;
mod settings;

fn main() -> cosmic::iced::Result {
    // Get the system's preferred languages.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    // Enable localizations to be applied.
    i18n::init(&requested_languages);

    // Starts the settings app's event loop
    cosmic::app::run::<settings::SettingsApp>(cosmic::app::Settings::default(), ())
}
