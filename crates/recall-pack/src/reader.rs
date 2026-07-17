use crate::error::{PackError, PackResult};
use crate::expand::ExpandView;
use crate::header::{PackHeader, HEADER_SIZE};
use crate::lex::LexView;
use crate::meta::MetaSection;
use crate::post::PostView;
use crate::pvec::PvecSection;
use crate::section::{SectionEntry, SectionTag, SECTION_ENTRY_SIZE};
use crate::text::TextSection;
use crate::{align_up, SECTION_ALIGN};
use std::collections::HashSet;

/// Zero-copy view over a pack file buffer.
#[derive(Debug)]
pub struct PackReader<'a> {
    bytes: &'a [u8],
    header: PackHeader,
    sections: Vec<SectionEntry>,
}

impl<'a> PackReader<'a> {
    pub fn open(bytes: &'a [u8]) -> PackResult<Self> {
        let header = PackHeader::parse(bytes)?;
        let table_start = HEADER_SIZE;
        let table_len = (header.section_count as usize)
            .checked_mul(SECTION_ENTRY_SIZE)
            .ok_or(PackError::Overflow)?;
        let table_end = table_start
            .checked_add(table_len)
            .ok_or(PackError::Overflow)?;
        if bytes.len() < table_end {
            return Err(PackError::TooShort {
                need: table_end,
                got: bytes.len(),
            });
        }
        let data_start = align_up(table_end, SECTION_ALIGN);

        let mut sections = Vec::with_capacity(header.section_count as usize);
        let mut seen_tags = HashSet::with_capacity(header.section_count as usize);
        for i in 0..header.section_count as usize {
            let off = table_start + i * SECTION_ENTRY_SIZE;
            let entry = SectionEntry::parse(&bytes[off..off + SECTION_ENTRY_SIZE])?;
            if entry.flags != 0
                || entry.offset % SECTION_ALIGN as u64 != 0
                || entry.offset < data_start as u64
                || !seen_tags.insert(entry.tag)
            {
                return Err(PackError::BadSectionTable);
            }
            let (_, end) = checked_section_bounds(entry.offset, entry.len)?;
            if end > bytes.len() {
                return Err(PackError::TruncatedSection {
                    tag: entry.tag.as_str(),
                });
            }
            sections.push(entry);
        }
        let mut ranges: Vec<(usize, usize)> = sections
            .iter()
            .map(|entry| checked_section_bounds(entry.offset, entry.len))
            .collect::<PackResult<_>>()?;
        ranges.sort_unstable();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(PackError::BadSectionTable);
        }

        Ok(Self {
            bytes,
            header,
            sections,
        })
    }

    pub fn header(&self) -> &PackHeader {
        &self.header
    }

    pub fn sections(&self) -> &[SectionEntry] {
        &self.sections
    }

    pub fn has(&self, tag: SectionTag) -> bool {
        self.sections.iter().any(|s| s.tag == tag)
    }

    pub fn section_bytes(&self, tag: SectionTag) -> PackResult<&'a [u8]> {
        let entry = self
            .sections
            .iter()
            .find(|s| s.tag == tag)
            .ok_or(PackError::MissingSection(tag.as_str()))?;
        let (start, end) = checked_section_bounds(entry.offset, entry.len)?;
        if end > self.bytes.len() {
            return Err(PackError::TruncatedSection {
                tag: entry.tag.as_str(),
            });
        }
        Ok(&self.bytes[start..end])
    }

    pub fn meta(&self) -> PackResult<MetaSection> {
        MetaSection::parse(self.section_bytes(SectionTag::Meta)?)
    }

    pub fn text(&self) -> PackResult<TextSection> {
        TextSection::parse(self.section_bytes(SectionTag::Text)?)
    }

    pub fn lex(&self) -> PackResult<LexView<'a>> {
        LexView::parse(self.section_bytes(SectionTag::Lex)?)
    }

    pub fn post(&self) -> PackResult<PostView<'a>> {
        PostView::parse(self.section_bytes(SectionTag::Post)?)
    }

    pub fn pvec(&self) -> PackResult<PvecSection> {
        PvecSection::parse(self.section_bytes(SectionTag::Pvec)?)
    }

    pub fn context_lex(&self) -> PackResult<LexView<'a>> {
        LexView::parse(self.section_bytes(SectionTag::Clex)?)
    }

    pub fn context_post(&self) -> PackResult<PostView<'a>> {
        PostView::parse(self.section_bytes(SectionTag::Cpost)?)
    }

    pub fn expand(&self) -> PackResult<ExpandView<'a>> {
        ExpandView::parse(self.section_bytes(SectionTag::Expand)?)
    }
}

fn checked_section_bounds(offset: u64, len: u64) -> PackResult<(usize, usize)> {
    let start = usize::try_from(offset).map_err(|_| PackError::Overflow)?;
    let length = usize::try_from(len).map_err(|_| PackError::Overflow)?;
    let end = start.checked_add(length).ok_or(PackError::Overflow)?;
    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::checked_section_bounds;
    use crate::PackError;

    #[test]
    fn section_bounds_reject_u64_arithmetic_overflow() {
        assert_eq!(
            checked_section_bounds(u64::MAX, 1),
            Err(PackError::Overflow)
        );
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn section_bounds_reject_values_larger_than_usize() {
        assert_eq!(
            checked_section_bounds(u32::MAX as u64 + 1, 0),
            Err(PackError::Overflow)
        );
    }
}
