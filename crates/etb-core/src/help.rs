//! The built-in guide.
//!
//! Content is authored as Markdown and embedded, so the guide works offline and
//! cannot get out of step with the build it ships in. The renderer lives in the
//! GUI crate; this module only supplies the text.

use crate::i18n::Lang;

pub struct HelpTopic {
    pub id: &'static str,
    pub title_vi: &'static str,
    pub title_en: &'static str,
    vi: &'static str,
    en: &'static str,
}

impl HelpTopic {
    pub fn title(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::Vi => self.title_vi,
            Lang::En => self.title_en,
        }
    }
    pub fn body(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::Vi => self.vi,
            Lang::En => self.en,
        }
    }
}

pub const TOPICS: &[HelpTopic] = &[
    HelpTopic {
        id: "01-bat-dau",
        title_vi: "Bắt đầu",
        title_en: "Getting started",
        vi: include_str!("../assets/help/vi/01-bat-dau.md"),
        en: include_str!("../assets/help/en/01-bat-dau.md"),
    },
    HelpTopic {
        id: "02-them-tep",
        title_vi: "Thêm tệp",
        title_en: "Adding files",
        vi: include_str!("../assets/help/vi/02-them-tep.md"),
        en: include_str!("../assets/help/en/02-them-tep.md"),
    },
    HelpTopic {
        id: "03-luu-va-chay",
        title_vi: "Lưu và chạy chương trình",
        title_en: "Saving and running",
        vi: include_str!("../assets/help/vi/03-luu-va-chay.md"),
        en: include_str!("../assets/help/en/03-luu-va-chay.md"),
    },
    HelpTopic {
        id: "04-loi-thuong-gap",
        title_vi: "Lỗi thường gặp",
        title_en: "Common problems",
        vi: include_str!("../assets/help/vi/04-loi-thuong-gap.md"),
        en: include_str!("../assets/help/en/04-loi-thuong-gap.md"),
    },
    HelpTopic {
        id: "05-canh-bao-bao-mat",
        title_vi: "Cảnh báo bảo mật",
        title_en: "Security warnings",
        vi: include_str!("../assets/help/vi/05-canh-bao-bao-mat.md"),
        en: include_str!("../assets/help/en/05-canh-bao-bao-mat.md"),
    },
    HelpTopic {
        id: "06-khac-voi-turbo-basic",
        title_vi: "Khác gì so với Turbo Basic",
        title_en: "How it differs from Turbo Basic",
        vi: include_str!("../assets/help/vi/06-khac-voi-turbo-basic.md"),
        en: include_str!("../assets/help/en/06-khac-voi-turbo-basic.md"),
    },
];

pub fn topic(id: &str) -> Option<&'static HelpTopic> {
    TOPICS.iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_has_content_in_both_languages() {
        for t in TOPICS {
            assert!(!t.vi.trim().is_empty(), "{} has no Vietnamese text", t.id);
            assert!(!t.en.trim().is_empty(), "{} has no English text", t.id);
            assert!(
                t.vi.starts_with('#'),
                "{} (vi) must open with a heading",
                t.id
            );
            assert!(
                t.en.starts_with('#'),
                "{} (en) must open with a heading",
                t.id
            );
        }
    }

    #[test]
    fn topic_ids_are_unique_and_findable() {
        let mut ids: Vec<_> = TOPICS.iter().map(|t| t.id).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate topic id");
        assert!(topic("01-bat-dau").is_some());
        assert!(topic("nope").is_none());
    }

    /// Strings that send the user to the guide must name a topic that exists.
    #[test]
    fn every_guide_reference_in_the_catalogs_resolves() {
        use crate::i18n;
        // `toolchain.missing.antivirus` promises a security topic.
        assert!(
            topic("05-canh-bao-bao-mat").is_some(),
            "the antivirus message points at a security topic that must exist"
        );
        for lang in [Lang::Vi, Lang::En] {
            let t = topic("05-canh-bao-bao-mat").unwrap();
            let body = t.body(lang).to_lowercase();
            assert!(
                body.contains("smartscreen") || body.contains("windows"),
                "the security topic must cover the Windows warning"
            );
            let _ = i18n::lookup(lang, "toolchain.missing.antivirus");
        }
    }

    #[test]
    fn the_guide_states_the_read_only_promise_in_both_languages() {
        // This promise is the product's core commitment; it must not vanish from
        // the guide in a future edit.
        let t = topic("01-bat-dau").unwrap();
        assert!(t.body(Lang::Vi).contains("không bao giờ sửa tệp"));
        assert!(t.body(Lang::En).contains("never changes your files"));
    }
}
