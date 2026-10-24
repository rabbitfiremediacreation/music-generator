//! Texte der Hülle in denselben zwölf Sprachen wie `static/i18n.js`. Fehlt ein Schlüssel: Englisch, dann Deutsch.

use std::collections::BTreeMap;

pub type Texts = BTreeMap<String, String>;

pub const LANGS: [(&str, &str); 12] = [
    ("de", include_str!("../../locales/de.json")),
    ("en", include_str!("../../locales/en.json")),
    ("es", include_str!("../../locales/es.json")),
    ("fr", include_str!("../../locales/fr.json")),
    ("it", include_str!("../../locales/it.json")),
    ("pt", include_str!("../../locales/pt.json")),
    ("tr", include_str!("../../locales/tr.json")),
    ("pl", include_str!("../../locales/pl.json")),
    ("ru", include_str!("../../locales/ru.json")),
    ("uk", include_str!("../../locales/uk.json")),
    ("ja", include_str!("../../locales/ja.json")),
    ("zh", include_str!("../../locales/zh.json")),
];

fn parse(lang: &str) -> Texts {
    LANGS
        .iter()
        .find(|(l, _)| *l == lang)
        .and_then(|(_, json)| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

pub fn is_supported(lang: &str) -> bool {
    LANGS.iter().any(|(l, _)| *l == lang)
}

/// Alle Texte einer Sprache, Lücken aus Englisch und Deutsch aufgefüllt.
pub fn texts(lang: &str) -> Texts {
    let mut t = parse("de");
    t.extend(parse("en"));
    if lang != "en" {
        t.extend(parse(lang));
    }
    t
}

pub fn t(lang: &str, key: &str) -> String {
    texts(lang).remove(key).unwrap_or_else(|| key.to_string())
}

/// Gespeicherte Wahl → Systemsprache → Englisch (wie `i18n.js`).
pub fn detect(saved: Option<&str>) -> String {
    if let Some(l) = saved.filter(|l| is_supported(l)) {
        return l.to_string();
    }
    let sys = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    let short = sys.get(..2).unwrap_or("");
    if is_supported(short) {
        short.to_string()
    } else {
        "en".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<String> {
        s.split('{')
            .skip(1)
            .filter_map(|p| p.split_once('}'))
            .map(|(n, _)| n.to_string())
            .collect()
    }

    #[test]
    fn all_languages_complete_and_consistent() {
        let de = parse("de");
        assert!(!de.is_empty(), "de.json ist leer oder kein gültiges JSON");
        for (lang, json) in LANGS {
            let t: Texts =
                serde_json::from_str(json).unwrap_or_else(|e| panic!("{lang}.json: {e}"));
            let missing: Vec<_> = de.keys().filter(|k| !t.contains_key(*k)).collect();
            let extra: Vec<_> = t.keys().filter(|k| !de.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{lang}.json fehlt: {missing:?}");
            assert!(
                extra.is_empty(),
                "{lang}.json hat unbekannte Schlüssel: {extra:?}"
            );
            for (k, v) in &t {
                assert!(!v.trim().is_empty(), "{lang}.json: {k} ist leer");
                assert_eq!(
                    placeholders(v),
                    placeholders(&de[k]),
                    "{lang}.json: Platzhalter in {k}"
                );
            }
        }
    }

    #[test]
    fn fallback_and_detect() {
        assert_eq!(t("de", "btn_quit"), "Beenden");
        assert_eq!(t("xx", "btn_quit"), "Quit");
        assert_eq!(t("de", "gibt_es_nicht"), "gibt_es_nicht");
        assert_eq!(detect(Some("ja")), "ja");
        assert!(is_supported(&detect(Some("xx"))));
    }
}
