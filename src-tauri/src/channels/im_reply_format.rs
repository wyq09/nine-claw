//! Shared IM reply shaping: same rules as `src/lib/replyCardFormat.ts`
//! (`nineclaw-cards` JSON fence, else `##` sections). Channel code turns
//! `ReplyCardItem` into plain-text or native payloads per platform.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplyCardTone {
    Default,
    Tip,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyCardItem {
    pub title: Option<String>,
    pub body: String,
    pub tone: ReplyCardTone,
}

/// Locate ```nineclaw-cards ... ``` ; returns byte range of entire fence (inclusive of both ```).
fn nineclaw_cards_fence_range(content: &str) -> Option<std::ops::Range<usize>> {
    let mut search = 0usize;
    while let Some(rel) = content[search..].find("```") {
        let fence_open = search + rel;
        let mut p = fence_open + 3;
        for (idx, c) in content[p..].char_indices() {
            if !c.is_whitespace() {
                p += idx;
                break;
            }
        }
        let tail = &content[p..];
        let kw = b"nineclaw-cards";
        let bytes = tail.as_bytes();
        if bytes.len() < kw.len() || !bytes[..kw.len()].eq_ignore_ascii_case(kw) {
            search = fence_open + 3;
            continue;
        }
        let mut q = p + kw.len();
        for c in content[q..].chars() {
            if !c.is_whitespace() {
                break;
            }
            q += c.len_utf8();
        }
        if let Some(rel_close) = content[q..].find("```") {
            let fence_close_end = q + rel_close + 3;
            return Some(fence_open..fence_close_end);
        }
        return None;
    }
    None
}

fn nineclaw_cards_json_inner(content: &str) -> Option<&str> {
    let range = nineclaw_cards_fence_range(content)?;
    let mut p = range.start + 3;
    for c in content[p..range.end].chars() {
        if !c.is_whitespace() {
            break;
        }
        p += c.len_utf8();
    }
    let tail = &content[p..range.end];
    let kw = "nineclaw-cards";
    if tail.len() < kw.len() || !tail[..kw.len()].eq_ignore_ascii_case(kw) {
        return None;
    }
    p += kw.len();
    for c in content[p..range.end].chars() {
        if !c.is_whitespace() {
            break;
        }
        p += c.len_utf8();
    }
    let close_rel = content[p..range.end].find("```")?;
    Some(content[p..p + close_rel].trim())
}

pub fn has_unclosed_nineclaw_cards_fence(content: &str) -> bool {
    let mut search = 0usize;
    while let Some(rel) = content[search..].find("```") {
        let fence_open = search + rel;
        let mut p = fence_open + 3;
        for (idx, c) in content[p..].char_indices() {
            if !c.is_whitespace() {
                p += idx;
                break;
            }
        }
        let tail = &content[p..];
        let kw = b"nineclaw-cards";
        let bytes = tail.as_bytes();
        if bytes.len() < kw.len() || !bytes[..kw.len()].eq_ignore_ascii_case(kw) {
            search = fence_open + 3;
            continue;
        }
        let mut q = p + kw.len();
        for c in content[q..].chars() {
            if !c.is_whitespace() {
                break;
            }
            q += c.len_utf8();
        }
        return !content[q..].contains("```");
    }
    false
}

fn strip_nineclaw_cards_block(content: &str) -> String {
    if let Some(r) = nineclaw_cards_fence_range(content) {
        format!("{}{}", &content[..r.start], &content[r.end..])
            .trim()
            .to_string()
    } else {
        content.to_string()
    }
}

fn parse_nineclaw_cards_block(content: &str) -> Option<Vec<ReplyCardItem>> {
    let json_str = nineclaw_cards_json_inner(content)?;
    let v: Value = serde_json::from_str(json_str).ok()?;
    let cards = v.get("cards")?.as_array()?;
    let mut out = Vec::new();
    for entry in cards {
        let obj = entry.as_object()?;
        let title = obj
            .get("title")
            .and_then(|x| x.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let body = obj
            .get("body")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let tone = match obj.get("tone").and_then(|x| x.as_str()) {
            Some("tip") => ReplyCardTone::Tip,
            Some("warning") => ReplyCardTone::Warning,
            _ => ReplyCardTone::Default,
        };
        let body_trim = body.trim();
        if body_trim.is_empty() && title.is_none() {
            continue;
        }
        out.push(ReplyCardItem {
            title,
            body: if body_trim.is_empty() {
                " ".to_string()
            } else {
                body_trim.to_string()
            },
            tone,
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn split_content_by_markdown_h2(content: &str) -> Vec<ReplyCardItem> {
    let text = content.trim();
    if text.is_empty() {
        return vec![ReplyCardItem {
            title: None,
            body: " ".to_string(),
            tone: ReplyCardTone::Default,
        }];
    }
    let mut cards: Vec<ReplyCardItem> = Vec::new();
    let mut current_title: Option<String> = None;
    let mut buf: Vec<&str> = Vec::new();

    let flush = |current_title: &mut Option<String>,
                 buf: &mut Vec<&str>,
                 cards: &mut Vec<ReplyCardItem>| {
        let body = buf.join("\n").trim().to_string();
        buf.clear();
        let title = current_title.take();
        if title.is_none() && body.is_empty() {
            return;
        }
        cards.push(ReplyCardItem {
            title,
            body: if body.is_empty() {
                " ".to_string()
            } else {
                body
            },
            tone: ReplyCardTone::Default,
        });
    };

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            flush(&mut current_title, &mut buf, &mut cards);
            current_title = Some(rest.trim().to_string());
        } else {
            buf.push(line);
        }
    }
    flush(&mut current_title, &mut buf, &mut cards);

    if cards.is_empty() {
        vec![ReplyCardItem {
            title: None,
            body: text.to_string(),
            tone: ReplyCardTone::Default,
        }]
    } else {
        cards
    }
}

/// Mirrors `resolveReplyCardItems` in `replyCardFormat.ts`.
pub fn resolve_reply_card_items(content: &str, is_streaming: bool) -> Vec<ReplyCardItem> {
    let trimmed = {
        let t = content.trim();
        if t.is_empty() {
            " ".to_string()
        } else {
            t.to_string()
        }
    };

    if is_streaming || has_unclosed_nineclaw_cards_fence(content) {
        return vec![ReplyCardItem {
            title: None,
            body: trimmed,
            tone: ReplyCardTone::Default,
        }];
    }

    if let Some(cards) = parse_nineclaw_cards_block(content) {
        return cards;
    }

    let rest = strip_nineclaw_cards_block(content);
    let source = if rest.trim().is_empty() {
        content
    } else {
        rest.as_str()
    };
    split_content_by_markdown_h2(source)
}

fn tone_line_prefix(tone: &ReplyCardTone) -> &'static str {
    match tone {
        ReplyCardTone::Tip => "💡 ",
        ReplyCardTone::Warning => "⚠️ ",
        ReplyCardTone::Default => "",
    }
}

fn format_one_wechat_card(item: &ReplyCardItem) -> String {
    let prefix = tone_line_prefix(&item.tone);
    let mut s = String::new();
    if let Some(ref t) = item.title {
        s.push_str("────────────\n");
        s.push_str(prefix);
        s.push_str(t);
        s.push_str("\n────────────\n");
        s.push_str(item.body.trim());
        s.push_str("\n────────────");
    } else if matches!(item.tone, ReplyCardTone::Tip | ReplyCardTone::Warning) {
        s.push_str("────────────\n");
        if !prefix.is_empty() {
            s.push_str(prefix.trim_end());
            s.push('\n');
        }
        s.push_str(item.body.trim());
        s.push_str("\n────────────");
    } else {
        s.push_str(item.body.trim());
    }
    s
}

/// One WeChat text message per segment; plain single block stays unframed.
pub fn wechat_im_text_segments(items: &[ReplyCardItem]) -> Vec<String> {
    if items.is_empty() {
        return Vec::new();
    }
    if items.len() == 1 {
        let one = &items[0];
        if one.title.is_none() && matches!(one.tone, ReplyCardTone::Default) {
            return vec![one.body.trim().to_string()];
        }
    }
    items.iter().map(|i| format_one_wechat_card(i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nineclaw_json_cards() {
        let s = r#"Hello
```nineclaw-cards
{ "cards": [ { "title": "A", "body": "Body one" }, { "body": "Only body" } ] }
```
tail"#;
        let v = resolve_reply_card_items(s, false);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].title.as_deref(), Some("A"));
        assert_eq!(v[0].body, "Body one");
        assert!(v[1].title.is_none());
        assert_eq!(v[1].body, "Only body");
    }

    #[test]
    fn h2_split() {
        let s = "## First\nalpha\n\n## Second\nbeta";
        let v = resolve_reply_card_items(s, false);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].title.as_deref(), Some("First"));
        assert_eq!(v[0].body, "alpha");
        assert_eq!(v[1].title.as_deref(), Some("Second"));
        assert_eq!(v[1].body, "beta");
    }

    #[test]
    fn wechat_single_plain_no_frame() {
        let items = resolve_reply_card_items("just text", false);
        let segs = wechat_im_text_segments(&items);
        assert_eq!(segs, vec!["just text"]);
    }

    #[test]
    fn wechat_multi_h2_two_messages() {
        let items = resolve_reply_card_items("## A\nx\n## B\ny", false);
        let segs = wechat_im_text_segments(&items);
        assert_eq!(segs.len(), 2);
        assert!(segs[0].contains("A"));
        assert!(segs[1].contains("B"));
    }
}
