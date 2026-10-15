//! Attachment URLs for thumbnails, built from `_wp_attached_file` and the size list in
//! `_wp_attachment_metadata`. Images are loaded by the UI straight from the site.

use super::php::{self, PhpKey, PhpValue};

const PREFERRED_SIZES: &[&str] = &[
    "woocommerce_thumbnail",
    "thumbnail",
    "medium",
    "woocommerce_gallery_thumbnail",
];

fn entry<'a>(items: &'a [(PhpKey, PhpValue)], key: &str) -> Option<&'a PhpValue> {
    items
        .iter()
        .find(|(k, _)| matches!(k, PhpKey::Str(s) if s == key.as_bytes()))
        .map(|(_, v)| v)
}

fn thumbnail_file(metadata: &str) -> Option<String> {
    let PhpValue::Array(meta) = php::parse(metadata.as_bytes()).ok()? else {
        return None;
    };
    let Some(PhpValue::Array(sizes)) = entry(&meta, "sizes") else {
        return None;
    };
    PREFERRED_SIZES
        .iter()
        .find_map(|size| match entry(sizes, size) {
            Some(PhpValue::Array(s)) => match entry(s, "file") {
                Some(PhpValue::Str(f)) => String::from_utf8(f.clone()).ok(),
                _ => None,
            },
            _ => None,
        })
}

/// `uploads` + `2025/10/mug.jpg` (+ metadata) → URL of a small rendition when one exists.
pub fn attachment_url(uploads: &str, attached_file: &str, metadata: Option<&str>) -> String {
    let file = attached_file.trim_start_matches('/');
    match metadata.and_then(thumbnail_file) {
        Some(thumb) => match file.rsplit_once('/') {
            Some((dir, _)) => format!("{uploads}/{dir}/{thumb}"),
            None => format!("{uploads}/{thumb}"),
        },
        None => format!("{uploads}/{file}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = r#"a:2:{s:4:"file";s:23:"2025/10/sWUQDRXxoAg.jpg";s:5:"sizes";a:2:{s:6:"medium";a:1:{s:4:"file";s:23:"sWUQDRXxoAg-300x200.jpg";}s:9:"thumbnail";a:1:{s:4:"file";s:23:"sWUQDRXxoAg-150x150.jpg";}}}"#;

    #[test]
    fn prefers_the_thumbnail_rendition() {
        assert_eq!(
            attachment_url(
                "https://shop.test/wp-content/uploads",
                "2025/10/sWUQDRXxoAg.jpg",
                Some(META)
            ),
            "https://shop.test/wp-content/uploads/2025/10/sWUQDRXxoAg-150x150.jpg"
        );
    }

    #[test]
    fn falls_back_to_the_original_file() {
        assert_eq!(
            attachment_url("https://shop.test/u", "2025/10/a.jpg", Some("broken")),
            "https://shop.test/u/2025/10/a.jpg"
        );
        assert_eq!(
            attachment_url("https://shop.test/u", "a.jpg", None),
            "https://shop.test/u/a.jpg"
        );
    }
}
