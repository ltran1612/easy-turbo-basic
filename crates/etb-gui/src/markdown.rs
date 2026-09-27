//! A very small Markdown renderer for the built-in guide.
//!
//! Deliberately not `egui_commonmark`: that crate tracks egui's release cadence,
//! and a version lag would block the guide on someone else's schedule. We author
//! the content, so we only need the subset we actually write — and the Markdown
//! files stay portable if we ever do swap it in.

pub fn render(ui: &mut egui::Ui, source: &str) {
    for raw in source.lines() {
        let line = raw.trim_end();

        if let Some(rest) = line.strip_prefix("### ") {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(rest).size(18.0).strong());
        } else if let Some(rest) = line.strip_prefix("## ") {
            ui.add_space(12.0);
            ui.label(egui::RichText::new(rest).size(21.0).strong());
            ui.add_space(2.0);
        } else if let Some(rest) = line.strip_prefix("# ") {
            ui.add_space(4.0);
            ui.heading(rest);
            ui.add_space(6.0);
        } else if let Some(rest) = line.strip_prefix("- ") {
            bullet(ui, rest);
        } else if let Some((num, rest)) = numbered(line) {
            bullet_with(ui, &format!("{num}."), rest);
        } else if line.is_empty() {
            ui.add_space(8.0);
        } else {
            inline(ui, line);
        }
    }
}

fn numbered(line: &str) -> Option<(u32, &str)> {
    let (head, rest) = line.split_once(". ")?;
    let n: u32 = head.trim().parse().ok()?;
    Some((n, rest))
}

fn bullet(ui: &mut egui::Ui, text: &str) {
    bullet_with(ui, "•", text);
}

fn bullet_with(ui: &mut egui::Ui, marker: &str, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(12.0);
        ui.label(egui::RichText::new(marker).strong());
        inline(ui, text);
    });
}

/// Handles `**bold**` and `` `code` `` within a paragraph.
fn inline(ui: &mut egui::Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (kind, piece) in split_inline(text) {
            let rt = egui::RichText::new(piece);
            match kind {
                Style::Bold => ui.label(rt.strong()),
                Style::Code => ui.label(rt.monospace()),
                Style::Italic => ui.label(rt.italics()),
                Style::Plain => ui.label(rt),
            };
        }
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Style {
    Plain,
    Bold,
    Code,
    Italic,
}

/// Split a paragraph into styled runs. Public for testing.
pub(crate) fn split_inline(text: &str) -> Vec<(Style, &str)> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let next = [
            ("**", Style::Bold),
            ("`", Style::Code),
            ("*", Style::Italic),
        ]
        .into_iter()
        .filter_map(|(delim, style)| rest.find(delim).map(|i| (i, delim, style)))
        .min_by_key(|(i, delim, _)| (*i, std::cmp::Reverse(delim.len())));

        let Some((start, delim, style)) = next else {
            out.push((Style::Plain, rest));
            break;
        };
        // A lone delimiter with no partner is literal text.
        let after = &rest[start + delim.len()..];
        let Some(end) = after.find(delim) else {
            out.push((Style::Plain, rest));
            break;
        };
        if start > 0 {
            out.push((Style::Plain, &rest[..start]));
        }
        out.push((style, &after[..end]));
        rest = &after[end + delim.len()..];
    }
    out.retain(|(_, s)| !s.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_one_run() {
        assert_eq!(
            split_inline("hello world"),
            vec![(Style::Plain, "hello world")]
        );
    }

    #[test]
    fn bold_wins_over_italic_for_a_double_star() {
        assert_eq!(
            split_inline("a **b** c"),
            vec![
                (Style::Plain, "a "),
                (Style::Bold, "b"),
                (Style::Plain, " c")
            ]
        );
    }

    #[test]
    fn code_spans_are_recognised() {
        assert_eq!(
            split_inline("use `INCLUDE 'X.INC'` here"),
            vec![
                (Style::Plain, "use "),
                (Style::Code, "INCLUDE 'X.INC'"),
                (Style::Plain, " here")
            ]
        );
    }

    #[test]
    fn an_unpaired_delimiter_is_literal_and_does_not_loop_forever() {
        assert_eq!(split_inline("2 * 3 = 6"), vec![(Style::Plain, "2 * 3 = 6")]);
        assert_eq!(split_inline("**oops"), vec![(Style::Plain, "**oops")]);
    }

    #[test]
    fn vietnamese_text_survives_splitting() {
        let runs = split_inline("Bấm **Biên dịch và chạy** để bắt đầu");
        assert_eq!(runs[1], (Style::Bold, "Biên dịch và chạy"));
    }

    #[test]
    fn numbered_list_items_are_detected() {
        assert_eq!(numbered("1. First step"), Some((1, "First step")));
        assert_eq!(numbered("not a list"), None);
    }

    #[test]
    fn every_shipped_help_topic_splits_without_panicking() {
        for t in etb_core::help::TOPICS {
            for lang in etb_core::i18n::Lang::all() {
                for line in t.body(lang).lines() {
                    let runs = split_inline(line);
                    let joined: String = runs.iter().map(|(_, s)| *s).collect();
                    assert!(
                        !joined.is_empty() || line.trim().is_empty(),
                        "lost text on line {line:?} of {}",
                        t.id
                    );
                }
            }
        }
    }
}
