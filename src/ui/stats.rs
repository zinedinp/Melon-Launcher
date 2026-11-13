//! The words for "last played" and "time played" under the account list.

use melon::i18n::Lang;

/// "just now", "5 min ago", "3 h ago", "2 d ago" for `secs` past.
pub fn ago(lang: &Lang, secs: u64) -> String {
    match secs {
        0..=59 => lang.get("melon.launcher.ago.now").to_owned(),
        60..=3599 => lang.format("melon.launcher.ago.minutes", &[&(secs / 60).to_string()]),
        3600..=86399 => lang.format("melon.launcher.ago.hours", &[&(secs / 3600).to_string()]),
        _ => lang.format("melon.launcher.ago.days", &[&(secs / 86400).to_string()]),
    }
}

/// "45 min" or "5 h 12 min" for a stretch of play.
pub fn span(lang: &Lang, secs: u64) -> String {
    let minutes = secs / 60;
    if minutes < 60 {
        lang.format("melon.launcher.duration.minutes", &[&minutes.to_string()])
    } else {
        lang.format(
            "melon.launcher.duration",
            &[&(minutes / 60).to_string(), &(minutes % 60).to_string()],
        )
    }
}
