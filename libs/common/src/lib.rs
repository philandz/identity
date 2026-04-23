use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceContext {
    pub org_id: String,
    pub user_id: String,
}

pub fn keyword_contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

pub fn flatten_tags(tags: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = tags.keys().collect();
    keys.sort();
    keys.iter()
        .map(|k| format!("{}={}", k, tags.get(*k).expect("must exist")))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_search_is_case_insensitive() {
        assert!(keyword_contains("Hello World", "world"));
    }

    #[test]
    fn flatten_tags_stable_order() {
        let mut map = HashMap::new();
        map.insert("b".to_string(), "2".to_string());
        map.insert("a".to_string(), "1".to_string());
        assert_eq!(flatten_tags(&map), "a=1,b=2");
    }
}
