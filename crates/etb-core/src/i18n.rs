//! Two languages and a few hundred strings do not earn Fluent's machinery.
//! Flat dotted keys, `{name}` interpolation, parsed once per language.
//!
//! Two tests guard this: key parity between the catalogs, and `xtask check-hygiene`
//! greps every `tr!` call site. Without them half the UI silently reverts to English.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Vi,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Vi => "vi",
            Lang::En => "en",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Lang::Vi => "Tiếng Việt",
            Lang::En => "English",
        }
    }
    pub fn all() -> [Lang; 2] {
        [Lang::Vi, Lang::En]
    }
    /// Follow the OS locale, falling back to Vietnamese. The user should never open the
    /// application and find it in English.
    pub fn from_locale(locale: Option<&str>) -> Lang {
        match locale {
            Some(l) if l.to_ascii_lowercase().starts_with("en") => Lang::En,
            _ => Lang::Vi,
        }
    }
}

const VI_TOML: &str = include_str!("../assets/i18n/vi.toml");
const EN_TOML: &str = include_str!("../assets/i18n/en.toml");

type Catalog = BTreeMap<String, String>;

fn catalog(lang: Lang) -> &'static Catalog {
    static VI: OnceLock<Catalog> = OnceLock::new();
    static EN: OnceLock<Catalog> = OnceLock::new();
    match lang {
        Lang::Vi => VI.get_or_init(|| parse(VI_TOML, "vi")),
        Lang::En => EN.get_or_init(|| parse(EN_TOML, "en")),
    }
}

fn parse(src: &str, which: &str) -> Catalog {
    toml::from_str(src).unwrap_or_else(|e| panic!("i18n catalog `{which}` is malformed: {e}"))
}

/// Look a key up. A missing key yields the key itself, which is ugly on screen and
/// therefore obvious in review — better than silently showing nothing.
pub fn lookup(lang: Lang, key: &str) -> String {
    if let Some(v) = catalog(lang).get(key) {
        return v.clone();
    }
    if lang != Lang::En {
        if let Some(v) = catalog(Lang::En).get(key) {
            return v.clone();
        }
    }
    key.to_string()
}

pub fn format(lang: Lang, key: &str, args: &[(&str, &str)]) -> String {
    let mut s = lookup(lang, key);
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

pub fn keys(lang: Lang) -> Vec<&'static str> {
    catalog(lang).keys().map(|s| s.as_str()).collect()
}

/// `tr!(lang, "key")` or `tr!(lang, "key", file = name, n = count)`.
#[macro_export]
macro_rules! tr {
    ($lang:expr, $key:literal) => {
        $crate::i18n::lookup($lang, $key)
    };
    ($lang:expr, $key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::format($lang, $key, &[$((stringify!($name), &::std::string::ToString::to_string(&$value))),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogs_have_identical_keys() {
        let vi: Vec<_> = keys(Lang::Vi);
        let en: Vec<_> = keys(Lang::En);
        let missing_en: Vec<_> = vi.iter().filter(|k| !en.contains(k)).collect();
        let missing_vi: Vec<_> = en.iter().filter(|k| !vi.contains(k)).collect();
        assert!(
            missing_en.is_empty() && missing_vi.is_empty(),
            "catalogs diverged.\n  missing from en.toml: {missing_en:?}\n  missing from vi.toml: {missing_vi:?}"
        );
    }

    #[test]
    fn catalogs_are_not_empty() {
        assert!(keys(Lang::Vi).len() > 20);
    }

    #[test]
    fn interpolation_substitutes_named_args() {
        let s = format(Lang::En, "programs.delete_confirm", &[("name", "Tính cọc")]);
        assert!(s.contains("Tính cọc"), "got {s:?}");
        assert!(
            !s.contains('{'),
            "a placeholder was left unsubstituted: {s:?}"
        );
    }

    #[test]
    fn vietnamese_is_the_fallback_locale() {
        assert_eq!(Lang::from_locale(Some("vi-VN")), Lang::Vi);
        assert_eq!(Lang::from_locale(Some("en-US")), Lang::En);
        assert_eq!(Lang::from_locale(Some("fr-FR")), Lang::Vi);
        assert_eq!(Lang::from_locale(None), Lang::Vi);
    }

    #[test]
    fn every_placeholder_exists_in_both_languages() {
        // If one language interpolates {file} and the other doesn't, one of them
        // silently loses information.
        for key in keys(Lang::Vi) {
            let vi = lookup(Lang::Vi, key);
            let en = lookup(Lang::En, key);
            let ph = |s: &str| -> Vec<String> {
                let mut v: Vec<String> = Vec::new();
                let mut rest = s;
                while let Some(i) = rest.find('{') {
                    if let Some(j) = rest[i..].find('}') {
                        v.push(rest[i + 1..i + j].to_string());
                        rest = &rest[i + j + 1..];
                    } else {
                        break;
                    }
                }
                v.sort();
                v
            };
            assert_eq!(ph(&vi), ph(&en), "placeholders differ for key `{key}`");
        }
    }
}
