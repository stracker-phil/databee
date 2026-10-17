//! Table prefixes: preset rules name tables without one, a project adds it.

use std::collections::HashSet;

/// The shortest prefix under which every one of `markers` exists among `tables`; ties go to
/// the alphabetically first. `None` when no prefix fits or there are no markers.
pub fn suggest_prefix<'a>(
    tables: impl IntoIterator<Item = &'a str>,
    markers: &[String],
) -> Option<String> {
    let tables: HashSet<&str> = tables.into_iter().collect();
    let first = markers.first()?;
    tables
        .iter()
        .filter_map(|table| table.strip_suffix(first.as_str()))
        .filter(|prefix| markers_exist(&tables, prefix, markers))
        .min_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)))
        .map(str::to_string)
}

pub fn markers_exist(tables: &HashSet<&str>, prefix: &str, markers: &[String]) -> bool {
    markers
        .iter()
        .all(|marker| tables.contains(format!("{prefix}{marker}").as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn markers(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn suggests_the_prefix_under_which_all_markers_exist() {
        let tables = ["wp_posts", "wp_options", "wp_users", "other"];
        assert_eq!(
            suggest_prefix(tables, &markers(&["options", "posts"])),
            Some("wp_".to_string())
        );
    }

    #[test]
    fn shortest_prefix_wins_over_a_longer_one() {
        let tables = ["xwp_posts", "xwp_options", "wp_posts", "wp_options"];
        assert_eq!(
            suggest_prefix(tables, &markers(&["posts", "options"])),
            Some("wp_".to_string())
        );
    }

    #[test]
    fn no_prefix_at_all_is_a_valid_suggestion() {
        let tables = ["wp_posts", "posts"];
        assert_eq!(
            suggest_prefix(tables, &markers(&["posts"])),
            Some(String::new())
        );
    }

    #[test]
    fn equally_short_prefixes_go_to_the_alphabetically_first() {
        let tables = ["b_posts", "a_posts"];
        assert_eq!(
            suggest_prefix(tables, &markers(&["posts"])),
            Some("a_".to_string())
        );
    }

    #[test]
    fn no_suggestion_when_a_marker_is_missing() {
        let tables = ["wp_posts", "wp_users"];
        assert_eq!(
            suggest_prefix(tables, &markers(&["posts", "options"])),
            None
        );
    }

    #[test]
    fn no_suggestion_when_markers_exist_under_different_prefixes() {
        let tables = ["a_posts", "b_options"];
        assert_eq!(
            suggest_prefix(tables, &markers(&["posts", "options"])),
            None
        );
    }

    #[test]
    fn no_suggestion_without_markers() {
        assert_eq!(suggest_prefix(["wp_posts"], &[]), None);
    }
}
