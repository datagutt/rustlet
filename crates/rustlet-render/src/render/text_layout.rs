use std::borrow::Cow;

use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

pub const INLINE_EMOJI_SIZE: i32 = 10;

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
        if looks_like_emoji_cluster(cluster) {
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

fn looks_like_emoji_cluster(cluster: &str) -> bool {
    let mut chars = cluster.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if cluster.contains('\u{200D}')
        || cluster.contains('\u{FE0F}')
        || cluster.contains('\u{20E3}')
        || is_regional_indicator(first)
    {
        return true;
    }

    if chars.clone().any(is_regional_indicator) {
        return true;
    }

    let code = first as u32;
    matches!(
        code,
        0x2600..=0x26FF
            | 0x2700..=0x27BF
            | 0x1F000..=0x1FAFF
    )
}

fn is_regional_indicator(ch: char) -> bool {
    matches!(ch as u32, 0x1F1E6..=0x1F1FF)
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
