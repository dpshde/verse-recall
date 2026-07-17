use recall_pack::{
    validate_lex_post, LexSection, LexTerm, MetaSection, PackReader, PackWriter, PostSection,
    PostTerm, Posting, SectionTag, TextSection,
};

fn sample_pack() -> Vec<u8> {
    let verses = vec![
        (
            "John 3:16".into(),
            "For God so loved the world that He gave His one and only Son".into(),
        ),
        (
            "Luke 15:4".into(),
            "What man among you, if he has a hundred sheep and loses one of them".into(),
        ),
        (
            "Psalm 23:1".into(),
            "The LORD is my shepherd; I shall not want.".into(),
        ),
    ];
    let text = TextSection::from_verses(verses);
    let meta = MetaSection::new("BSB", text.verses.len() as u32);

    PackWriter::new().add_meta(&meta).add_text(&text).finish()
}

#[test]
fn meta_text_roundtrip() {
    let bytes = sample_pack();
    assert!(bytes.len() >= 64);
    assert_eq!(&bytes[0..8], b"VRCALL01");

    let reader = PackReader::open(&bytes).expect("open pack");
    assert!(reader.has(SectionTag::Meta));
    assert!(reader.has(SectionTag::Text));
    assert!(!reader.has(SectionTag::Wvec));

    let meta = reader.meta().expect("meta");
    assert_eq!(meta.translation_id, "BSB");
    assert_eq!(meta.verse_count, 3);

    let text = reader.text().expect("text");
    assert_eq!(text.verses.len(), 3);
    assert_eq!(text.verses[0].reference, "John 3:16");
    assert!(text.verses[1].text.contains("sheep"));
    assert_eq!(text.verses[2].reference, "Psalm 23:1");
}

#[test]
fn rejects_bad_magic() {
    let mut bytes = sample_pack();
    bytes[0] = b'X';
    assert!(PackReader::open(&bytes).is_err());
}

#[test]
fn rejects_truncated() {
    assert!(PackReader::open(&[0u8; 10]).is_err());
}

#[test]
fn rejects_unverified_hash_claims_and_reserved_hash_bytes() {
    let mut flagged = sample_pack();
    flagged[12..16].copy_from_slice(&1u32.to_le_bytes());
    assert!(matches!(
        PackReader::open(&flagged),
        Err(recall_pack::PackError::UnsupportedFlags(1))
    ));

    let mut hash_without_flag = sample_pack();
    hash_without_flag[24] = 1;
    assert!(matches!(
        PackReader::open(&hash_without_flag),
        Err(recall_pack::PackError::UnsupportedFlags(1))
    ));
}

#[test]
fn rejects_nonzero_reserved_fields_and_tag_padding() {
    let mut header_reserved0 = sample_pack();
    header_reserved0[20] = 1;
    assert!(matches!(
        PackReader::open(&header_reserved0),
        Err(recall_pack::PackError::BadSectionTable)
    ));

    let mut header_reserved1 = sample_pack();
    header_reserved1[56] = 1;
    assert!(matches!(
        PackReader::open(&header_reserved1),
        Err(recall_pack::PackError::BadSectionTable)
    ));

    let mut section_reserved = sample_pack();
    section_reserved[64 + 28] = 1;
    assert!(matches!(
        PackReader::open(&section_reserved),
        Err(recall_pack::PackError::BadSectionTable)
    ));

    let mut dirty_tag_padding = sample_pack();
    dirty_tag_padding[64 + 5] = b'X';
    assert!(matches!(
        PackReader::open(&dirty_tag_padding),
        Err(recall_pack::PackError::BadSectionTable)
    ));
}

#[test]
fn rejects_unsupported_meta_versions_and_invalid_weights() {
    let mut unsupported = MetaSection::new("BSB", 1).encode();
    unsupported[0..4].copy_from_slice(&2u32.to_le_bytes());
    assert!(matches!(
        MetaSection::parse(&unsupported),
        Err(recall_pack::PackError::UnsupportedMetaVersion(2))
    ));

    let mut invalid_weight = MetaSection::new("BSB", 1);
    invalid_weight.weight_a = f32::INFINITY;
    assert!(matches!(
        MetaSection::parse(&invalid_weight.encode()),
        Err(recall_pack::PackError::InvalidMeta)
    ));

    let encoded = MetaSection::new("BSB", 1).encode();
    let mut partial_weights = encoded.clone();
    partial_weights.pop();
    assert!(matches!(
        MetaSection::parse(&partial_weights),
        Err(recall_pack::PackError::InvalidMeta)
    ));

    let mut dirty_alignment_padding = encoded.clone();
    dirty_alignment_padding[13] = 1;
    assert!(matches!(
        MetaSection::parse(&dirty_alignment_padding),
        Err(recall_pack::PackError::InvalidMeta)
    ));

    let mut trailing_byte = encoded.clone();
    trailing_byte.push(0);
    assert!(matches!(
        MetaSection::parse(&trailing_byte),
        Err(recall_pack::PackError::InvalidMeta)
    ));

    let parsed = MetaSection::parse(&encoded[..13]).expect("v1 weights may be absent");
    assert_eq!(parsed.weight_a, 0.0);
}

#[test]
fn rejects_duplicate_or_overlapping_section_entries() {
    let mut duplicate = sample_pack();
    let first_tag = duplicate[64..72].to_vec();
    duplicate[96..104].copy_from_slice(&first_tag);
    assert!(matches!(
        PackReader::open(&duplicate),
        Err(recall_pack::PackError::BadSectionTable)
    ));

    let mut overlapping = sample_pack();
    let first_offset = overlapping[72..80].to_vec();
    overlapping[104..112].copy_from_slice(&first_offset);
    assert!(matches!(
        PackReader::open(&overlapping),
        Err(recall_pack::PackError::BadSectionTable)
    ));
}

#[test]
fn text_parser_rejects_duplicate_normalized_refs() {
    let text = TextSection::from_verses(vec![
        ("Genesis 1:1".into(), "One".into()),
        ("Genesis 1.1".into(), "Duplicate".into()),
    ]);
    let meta = MetaSection::new("BSB", 2);
    let pack = PackWriter::new().add_meta(&meta).add_text(&text).finish();
    let reader = PackReader::open(&pack).expect("section framing remains valid");
    assert!(matches!(
        reader.text(),
        Err(recall_pack::PackError::InvalidText)
    ));
}

#[test]
fn pack_reader_exposes_aligned_lex_post_views() {
    let lex = LexSection::new(
        2,
        vec![LexTerm {
            text: "shepherd".into(),
            document_frequency: 1,
        }],
    )
    .unwrap();
    let post = PostSection::new(
        vec![5, 7],
        vec![PostTerm {
            postings: vec![Posting {
                document_id: 1,
                term_frequency: 2,
            }],
        }],
    )
    .unwrap();
    let pack = PackWriter::new().add_lex(&lex).add_post(&post).finish();
    let reader = PackReader::open(&pack).unwrap();
    let lex_view = reader.lex().unwrap();
    let post_view = reader.post().unwrap();
    validate_lex_post(lex_view, post_view).unwrap();
    assert_eq!(lex_view.lookup("shepherd"), Some(0));
    assert_eq!(
        post_view.postings(0).unwrap().next().unwrap().document_id,
        1
    );
}

#[test]
fn context_lex_post_roundtrip_uses_distinct_tags_and_shared_formats() {
    let lex = LexSection::new(
        2,
        vec![LexTerm {
            text: "incarn".into(),
            document_frequency: 1,
        }],
    )
    .unwrap();
    let post = PostSection::new(
        vec![9, 4],
        vec![PostTerm {
            postings: vec![Posting {
                document_id: 0,
                term_frequency: 1,
            }],
        }],
    )
    .unwrap();
    let pack = PackWriter::new()
        .add_context_lex(&lex)
        .add_context_post(&post)
        .finish();
    let reader = PackReader::open(&pack).unwrap();

    assert!(!reader.has(SectionTag::Lex));
    assert!(!reader.has(SectionTag::Post));
    assert!(reader.has(SectionTag::Clex));
    assert!(reader.has(SectionTag::Cpost));
    let lex_view = reader.context_lex().unwrap();
    let post_view = reader.context_post().unwrap();
    validate_lex_post(lex_view, post_view).unwrap();
    assert_eq!(lex_view.lookup("incarn"), Some(0));
    assert_eq!(
        post_view.postings(0).unwrap().next().unwrap().document_id,
        0
    );
}

#[test]
fn malformed_context_sections_fail_typed_parsing() {
    let pack = PackWriter::new()
        .add_raw(SectionTag::Clex, b"not-a-lex-section".to_vec())
        .add_raw(SectionTag::Cpost, b"not-a-post-section".to_vec())
        .finish();
    let reader = PackReader::open(&pack).expect("section framing remains valid");

    assert!(matches!(
        reader.context_lex(),
        Err(recall_pack::PackError::InvalidLex)
    ));
    assert!(matches!(
        reader.context_post(),
        Err(recall_pack::PackError::InvalidPost)
    ));
}
