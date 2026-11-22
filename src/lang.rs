//! The launcher's own texts, in `lang/<code>.json`.

use melon_base::i18n::languages::Language;
use std::collections::HashMap;

const EN: &str = include_str!("../lang/en_us.json");
const DE: &str = include_str!("../lang/de_de.json");

/// The languages the launcher has texts for.
pub const LANGUAGES: [Language; 2] = [Language::EnUs, Language::DeDe];

pub struct Strings {
    language: Language,
    texts: HashMap<String, String>,
    /// English, for keys the language lacks.
    fallback: HashMap<String, String>,
}

fn parse(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).unwrap_or_default()
}

impl Strings {
    pub fn new(language: Language) -> Self {
        let texts = match language {
            Language::DeDe => parse(DE),
            _ => parse(EN),
        };
        Self {
            language,
            texts,
            fallback: parse(EN),
        }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// The text for `key`; the key itself when nothing has it.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.texts
            .get(key)
            .or_else(|| self.fallback.get(key))
            .map_or(key, String::as_str)
    }

    /// The text for `key` with each `%s` replaced by the next argument.
    pub fn format(&self, key: &str, args: &[&str]) -> String {
        let mut out = String::new();
        let mut args = args.iter();
        let mut rest = self.get(key);
        while let Some(at) = rest.find("%s") {
            out.push_str(&rest[..at]);
            out.push_str(args.next().copied().unwrap_or_default());
            rest = &rest[at + 2..];
        }
        out.push_str(rest);
        out
    }

    /// "just now", "5 min ago", "3 h ago", "2 d ago" for `secs` past.
    pub fn ago(&self, secs: u64) -> String {
        match secs {
            0..=59 => self.get("time.now").to_owned(),
            60..=3599 => self.format("time.minutes_ago", &[&(secs / 60).to_string()]),
            3600..=86399 => self.format("time.hours_ago", &[&(secs / 3600).to_string()]),
            _ => self.format("time.days_ago", &[&(secs / 86400).to_string()]),
        }
    }

    /// "45 min" or "5 h 12 min" for a stretch of play.
    pub fn span(&self, secs: u64) -> String {
        let minutes = secs / 60;
        if minutes < 60 {
            self.format("time.minutes", &[&minutes.to_string()])
        } else {
            self.format(
                "time.hours_minutes",
                &[&(minutes / 60).to_string(), &(minutes % 60).to_string()],
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_languages_have_the_same_keys() {
        let (en, de) = (parse(EN), parse(DE));
        assert!(!en.is_empty());
        let mut missing: Vec<_> = en.keys().filter(|k| !de.contains_key(*k)).collect();
        missing.extend(de.keys().filter(|k| !en.contains_key(*k)));
        assert!(missing.is_empty(), "keys in one language only: {missing:?}");
    }

    #[test]
    fn arguments_fill_in_order() {
        let strings = Strings::new(Language::EnUs);
        assert_eq!(strings.format("time.hours_minutes", &["5", "12"]), "5 h 12 min");
        assert_eq!(strings.get("no.such.key"), "no.such.key");
    }
}
