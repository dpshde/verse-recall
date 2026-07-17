use crate::align_up;
use crate::expand::ExpandSection;
use crate::header::PackHeader;
use crate::lex::LexSection;
use crate::meta::MetaSection;
use crate::post::PostSection;
use crate::pvec::PvecSection;
use crate::section::{SectionEntry, SectionTag, SECTION_ENTRY_SIZE};
use crate::text::TextSection;
use crate::{HEADER_SIZE, SECTION_ALIGN};

/// Builds a pack with META + TEXT (and optional raw sections later).
#[derive(Debug, Default)]
pub struct PackWriter {
    sections: Vec<(SectionTag, Vec<u8>)>,
}

impl PackWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_meta(&mut self, meta: &MetaSection) -> &mut Self {
        self.sections.push((SectionTag::Meta, meta.encode()));
        self
    }

    pub fn add_text(&mut self, text: &TextSection) -> &mut Self {
        self.sections.push((SectionTag::Text, text.encode()));
        self
    }

    pub fn add_lex(&mut self, lex: &LexSection) -> &mut Self {
        self.sections.push((SectionTag::Lex, lex.encode()));
        self
    }

    pub fn add_post(&mut self, post: &PostSection) -> &mut Self {
        self.sections.push((SectionTag::Post, post.encode()));
        self
    }

    pub fn add_pvec(&mut self, pvec: &PvecSection) -> &mut Self {
        self.sections.push((
            SectionTag::Pvec,
            pvec.encode().expect("validated PVEC section"),
        ));
        self
    }

    /// Add a lexical dictionary whose document ids align one-for-one with
    /// canonical TEXT, while indexing only auxiliary context prose.
    pub fn add_context_lex(&mut self, lex: &LexSection) -> &mut Self {
        self.sections.push((SectionTag::Clex, lex.encode()));
        self
    }

    /// Add postings paired with CLEX. CLEX/CPOST intentionally reuse the
    /// validated lexical wire formats; distinct tags keep them from being
    /// mistaken for canonical wording evidence.
    pub fn add_context_post(&mut self, post: &PostSection) -> &mut Self {
        self.sections.push((SectionTag::Cpost, post.encode()));
        self
    }

    pub fn add_expand(&mut self, expand: &ExpandSection) -> &mut Self {
        self.sections.push((SectionTag::Expand, expand.encode()));
        self
    }

    pub fn add_raw(&mut self, tag: SectionTag, body: Vec<u8>) -> &mut Self {
        self.sections.push((tag, body));
        self
    }

    pub fn finish(&self) -> Vec<u8> {
        let n = self.sections.len() as u32;
        let header = PackHeader::new(n);
        let table_len = self.sections.len() * SECTION_ENTRY_SIZE;
        let data_start = align_up(HEADER_SIZE + table_len, SECTION_ALIGN);

        // First pass: compute offsets
        let mut entries = Vec::with_capacity(self.sections.len());
        let mut cursor = data_start;
        for (tag, body) in &self.sections {
            let offset = cursor as u64;
            let len = body.len() as u64;
            entries.push(SectionEntry {
                tag: *tag,
                offset,
                len,
                flags: 0,
            });
            cursor = align_up(cursor + body.len(), SECTION_ALIGN);
        }

        let mut out = Vec::with_capacity(cursor);
        header.write_to(&mut out);
        debug_assert_eq!(out.len(), HEADER_SIZE);
        for e in &entries {
            e.write_to(&mut out);
        }
        // Pad to data_start
        while out.len() < data_start {
            out.push(0);
        }

        for ((_, body), entry) in self.sections.iter().zip(entries.iter()) {
            debug_assert_eq!(out.len(), entry.offset as usize);
            out.extend_from_slice(body);
            let padded = align_up(out.len(), SECTION_ALIGN);
            while out.len() < padded {
                out.push(0);
            }
        }

        out
    }
}
