use std::borrow::Cow;

use super::emoji_atlas;
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSegment {
    Text(String),
    Emoji(String),
}

pub fn base_direction_is_rtl(text: &str) -> bool {
    BidiInfo::new(text, None)
        .paragraphs
        .first()
        .map(|para| para.level.is_rtl())
        .unwrap_or(false)
}

pub fn visual_bidi_string(text: &str) -> Cow<'_, str> {
    if text.is_empty() {
        return Cow::Borrowed(text);
    }

    let info = BidiInfo::new(text, None);
    let Some(para) = info.paragraphs.first() else {
        return Cow::Borrowed(text);
    };

    info.reorder_line(para, para.range.clone())
}

pub fn segment_string(text: &str) -> (Vec<TextSegment>, bool) {
    let mut has_emoji = false;
    let mut segments = Vec::new();
    let mut text_run = String::new();

    for cluster in text.graphemes(true) {
        if emoji_atlas::contains_exact(cluster) {
            if !text_run.is_empty() {
                segments.push(TextSegment::Text(std::mem::take(&mut text_run)));
            }
            has_emoji = true;
            segments.push(TextSegment::Emoji(cluster.to_string()));
        } else {
            text_run.push_str(cluster);
        }
    }

    if !text_run.is_empty() {
        segments.push(TextSegment::Text(text_run));
    }

    (segments, has_emoji)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_direction_matches_pixlet_cases() {
        assert!(!base_direction_is_rtl("Hello, world"));
        assert!(base_direction_is_rtl("שלום"));
        assert!(base_direction_is_rtl("123 שלום"));
        assert!(!base_direction_is_rtl("12345!?."));
    }

    #[test]
    fn visual_bidi_string_matches_reference_examples() {
        assert_eq!(visual_bidi_string(""), "");
        assert_eq!(visual_bidi_string("Pixlet"), "Pixlet");
        assert_eq!(visual_bidi_string("שלום"), "םולש");
        assert_eq!(visual_bidi_string("abc שלום def"), "abc םולש def");
        assert_eq!(visual_bidi_string("שלום abc"), "abc םולש");
    }

    #[test]
    fn segment_string_detects_emoji_clusters() {
        assert_eq!(
            segment_string("Hello 😀 World"),
            (
                vec![
                    TextSegment::Text("Hello ".to_string()),
                    TextSegment::Emoji("😀".to_string()),
                    TextSegment::Text(" World".to_string()),
                ],
                true,
            )
        );
        assert_eq!(
            segment_string("↗️"),
            (vec![TextSegment::Emoji("↗️".to_string())], true)
        );
        assert_eq!(
            segment_string("123456"),
            (vec![TextSegment::Text("123456".to_string())], false)
        );
    }
}
