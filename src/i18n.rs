//! English message IDs, named placeholders, and a compiled catalog boundary.
//! Package descriptions and package-manager diagnostics remain source data.
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

const CATALOGS: &[(&str, &str)] = &[("en", include_str!("../locales/en.json"))];
fn catalog() -> &'static HashMap<String, String> {
    static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let locale = std::env::var("PACSTORE_LANGUAGE")
            .or_else(|_| std::env::var("LC_ALL"))
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_else(|_| "en".into());
        let language = locale.split(['_', '-', '.']).next().unwrap_or("en");
        let source = CATALOGS
            .iter()
            .find(|(name, _)| *name == language)
            .unwrap_or(&CATALOGS[0])
            .1;
        serde_json::from_str::<HashMap<String, String>>(source)
            .unwrap_or_default()
            .into_iter()
            .filter(|(key, value)| placeholders(key) == placeholders(value))
            .collect()
    })
}
pub fn text(message: &'static str) -> &'static str {
    catalog()
        .get(message)
        .map(String::as_str)
        .unwrap_or(message)
}
fn placeholders(message: &str) -> HashSet<&str> {
    message
        .split('{')
        .skip(1)
        .filter_map(|tail| tail.split_once('}').map(|(key, _)| key))
        .collect()
}
pub fn render(template: &'static str, values: &[(&str, String)]) -> String {
    substitute(text(template), values)
}
fn substitute(template: &str, values: &[(&str, String)]) -> String {
    let mut output = String::new();
    let mut remaining = template;
    while let Some((prefix, tail)) = remaining.split_once('{') {
        output.push_str(prefix);
        let Some((name, rest)) = tail.split_once('}') else {
            output.push('{');
            output.push_str(tail);
            return output;
        };
        if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
            output.push_str(value);
        } else {
            output.push('{');
            output.push_str(name);
            output.push('}');
        }
        remaining = rest;
    }
    output.push_str(remaining);
    output
}
#[macro_export]
macro_rules! t {
    ($message:expr) => {
        $crate::i18n::text($message)
    };
}
#[macro_export]
macro_rules! trf {
    ($message:literal, $($key:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::render($message, &[$((stringify!($key), ($value).to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_values_can_be_reordered_and_are_not_interpreted_as_templates() {
        assert_eq!(
            substitute(
                "{version} : {name}",
                &[("name", "{version}".into()), ("version", "2".into())]
            ),
            "2 : {version}"
        );
        assert_eq!(text("Untranslated fallback"), "Untranslated fallback");
    }
    #[test]
    fn shipped_catalog_preserves_placeholders() {
        for (_, source) in CATALOGS {
            for (key, value) in serde_json::from_str::<HashMap<String, String>>(source).unwrap() {
                assert_eq!(placeholders(&key), placeholders(&value), "{key}");
            }
        }
    }
}
