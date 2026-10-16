//! Attachment URLs for thumbnails, built from `_wp_attached_file` and the size list in
//! `_wp_attachment_metadata`. Images are loaded by the UI straight from the site.

use crate::decode::php::{self, PhpValue};

const PREFERRED_SIZES: &[&str] = &[
    "woocommerce_thumbnail",
    "thumbnail",
    "medium",
    "woocommerce_gallery_thumbnail",
];

fn thumbnail_file(metadata: &str) -> Option<String> {
    let meta = php::parse(metadata.as_bytes()).ok()?;
    let sizes = meta.get("sizes")?;
    PREFERRED_SIZES
        .iter()
        .find_map(|size| match sizes.get(size)?.get("file")? {
            PhpValue::Str(f) => String::from_utf8(f.clone()).ok(),
            _ => None,
        })
}

/// The uploads base URL an attachment's `guid` proves: the guid without its `_wp_attached_file`
/// path, e.g. `https://shop.test/wp-content/uploads/2025/10/mug.jpg` with `2025/10/mug.jpg` →
/// `https://shop.test/wp-content/uploads`. `None` when the guid does not end with that path.
pub fn uploads_base_from_guid<'a>(guid: &'a str, attached_file: &str) -> Option<&'a str> {
    let file = attached_file.trim_start_matches('/');
    if file.is_empty() {
        return None;
    }
    guid.strip_suffix(file)?.strip_suffix('/')
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
    fn the_guid_proves_the_uploads_base() {
        for (guid, file, base) in [
            (
                "https://shop.test/wp-content/uploads/2025/10/mug.jpg",
                "2025/10/mug.jpg",
                "https://shop.test/wp-content/uploads",
            ),
            (
                "https://shop.test/wp-content/uploads/2025/10/mug.jpg",
                "/2025/10/mug.jpg",
                "https://shop.test/wp-content/uploads",
            ),
            (
                "https://shop.test/sites/2/mug.jpg",
                "mug.jpg",
                "https://shop.test/sites/2",
            ),
        ] {
            assert_eq!(uploads_base_from_guid(guid, file), Some(base), "{guid}");
        }
    }

    #[test]
    fn a_guid_that_does_not_end_with_the_file_proves_nothing() {
        for (guid, file) in [
            ("https://shop.test/?attachment_id=124", "2025/10/mug.jpg"),
            (
                "https://shop.test/wp-content/uploads/2025/10/other.jpg",
                "2025/10/mug.jpg",
            ),
            ("https://shop.test/wp-content/uploads/2025/10/mug.jpg", ""),
            ("https://shop.test/wp-content/uploads/2025/10/mug.jpg", "/"),
            ("https://shop.test/uploadsmug.jpg", "mug.jpg"),
            ("mug.jpg", "mug.jpg"),
        ] {
            assert_eq!(uploads_base_from_guid(guid, file), None, "{guid} / {file}");
        }
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
