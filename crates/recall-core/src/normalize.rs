/// Normalized query tokens for lane dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedQuery {
    /// Original query (trimmed).
    pub raw: String,
    /// Lowercased alphanumeric tokens (surface forms, stop-stripped).
    pub terms: Vec<String>,
    /// Light stems / lemmas for matching (when LEX absent).
    pub lemmas: Vec<String>,
}

/// Identity of the query/corpus stemmer used by lexical and expansion packs.
///
/// Any behavior change to [`stem`] requires incrementing this value and
/// rebuilding every LEX/CLEX/EXPAND artifact. The compile-time assertion keeps
/// the runtime stemmer and the on-disk section parsers on the same identity.
pub const NORMALIZER_VERSION: u16 = 2;
const _: () = assert!(NORMALIZER_VERSION == recall_pack::LEX_NORMALIZER_VERSION);

/// Tiny English stop set — enough for scaffold TEXT scan, not a full NLP stack.
const STOP: &[&str] = &[
    "a",
    "an",
    "the",
    "and",
    "or",
    "but",
    "if",
    "of",
    "to",
    "in",
    "on",
    "for",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "being",
    "that",
    "this",
    "it",
    "he",
    "she",
    "they",
    "we",
    "you",
    "i",
    "his",
    "her",
    "their",
    "my",
    "your",
    "with",
    "as",
    "by",
    "from",
    "at",
    "not",
    "no",
    "so",
    "one",
    "only",
    "about",
    "verse",
    "verses",
    "passage",
    "passages",
    "scripture",
    "bible",
    "guy",
    "man",
    "people",
    "person",
    "someone",
    "where",
    "when",
    "who",
    "what",
    "how",
    "which",
    "there",
    "here",
    "into",
    "over",
    "under",
    "than",
    "then",
    "also",
    "just",
    "very",
    "much",
    "more",
    "some",
    "any",
    "all",
    "its",
    "our",
    "does",
    "did",
    "do",
    "has",
    "have",
    "had",
    "will",
    "would",
    "could",
    "should",
    "can",
    "may",
    "might",
    "shall",
    "unto",
    "ye",
    "thou",
    "thy",
    "thee",
];

/// Normalize a messy TOT query into terms + light stems.
///
/// Full lemmatization is pack-driven (LEX section). Scaffold applies a tiny
/// English stemmer so "loving"/"loved" and "giving"/"gave" align.
pub fn normalize_query(query: &str) -> NormalizedQuery {
    let raw = query.trim().to_string();
    // British / archaic spelling folded before tokenization.
    let folded = raw
        .to_lowercase()
        .replace("armour", "armor")
        .replace("favour", "favor")
        .replace("honour", "honor")
        .replace("saviour", "savior")
        .replace("neighbour", "neighbor");
    let mut terms = Vec::new();
    let mut current = String::new();

    for ch in folded.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            push_term(&mut terms, &current);
            current.clear();
        }
    }
    if !current.is_empty() {
        push_term(&mut terms, &current);
    }

    let lemmas: Vec<String> = terms.iter().map(|t| stem(t)).collect();
    NormalizedQuery { raw, terms, lemmas }
}

/// Tokenize verse text the same way queries are tokenized (no stop filter).
pub fn tokenize_text(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            if current.len() > 1 {
                terms.push(current.clone());
            }
            current.clear();
        }
    }
    if current.len() > 1 {
        terms.push(current);
    }
    terms
}

/// Light English stem used for matching surfaces across morphology.
pub fn stem(t: &str) -> String {
    // Irregular / biblical high-value pairs first.
    match t {
        "lost" | "loses" | "lose" | "losing" | "losts" => return "lose".into(),
        "loved" | "loves" | "loving" | "love" => return "love".into(),
        "gave" | "gives" | "giving" | "give" | "given" => return "give".into(),
        "found" | "finds" | "finding" | "find" => return "find".into(),
        "left" | "leaves" | "leaving" | "leave" => return "leave".into(),
        "blessed" | "blesses" | "blessing" | "bless" => return "bless".into(),
        "believes" | "believed" | "believing" | "believe" | "believers" | "believer" => {
            return "believe".into()
        }
        "created" | "creates" | "creating" | "create" | "creation" => return "create".into(),
        "made" | "make" | "makes" | "making" => return "make".into(),
        "shepherds" | "shepherding" | "shepherded" | "shepherd" => return "shepherd".into(),
        "sheep" | "sheeps" => return "sheep".into(),
        "sons" | "son" => return "son".into(),
        "worlds" | "world" => return "world".into(),
        "spirits" | "spirit" => return "spirit".into(),
        "moneys" | "money" => return "money".into(),
        "evils" | "evil" => return "evil".into(),
        "roots" | "root" => return "root".into(),
        "poor" => return "poor".into(),
        "begotten" => return "begot".into(),
        "ninety" | "ninty" => return "ninety".into(),
        "hundred" => return "hundred".into(),
        "parable" | "parables" => return "parable".into(),
        "kingdom" | "kingdoms" => return "kingdom".into(),
        "heaven" | "heavens" => return "heaven".into(),
        "eternal" | "eternally" => return "eternal".into(),
        "perish" | "perished" | "perishes" | "perishing" => return "perish".into(),
        "walking" | "walked" | "walks" | "walk" => return "walk".into(),
        "scared" | "scares" | "scaring" | "scare" => return "scare".into(),
        "dangerous" | "danger" => return "danger".into(),
        "darkness" | "dark" => return "dark".into(),
        "qualities" | "quality" => return "quality".into(),
        "faithful" | "faithfully" | "faithfulness" => return "faithful".into(),
        "angry" | "anger" => return "anger".into(),
        "produces" | "produced" | "producing" | "produce" => return "produce".into(),
        "everything" | "everyone" | "everyone's" => return t.to_string(),
        "beginning" | "begin" | "began" | "begun" => return "begin".into(),
        "starts" | "started" | "starting" | "start" => return "start".into(),
        "armor" | "armour" => return "armor".into(),
        "psalm" | "psalms" => return "psalm".into(),
        _ => {}
    }

    // Do not stem closed-class-ish long tokens that look like content words
    // ending in ing/ous/ness — aggressive rules destroy them (everything→everyth).
    if t.ends_with("thing")
        || t.ends_with("ness")
        || t.ends_with("ous")
        || t.ends_with("tion")
        || t.ends_with("sion")
        || t.ends_with("ment")
        || t.ends_with("ity")
        || t.ends_with("ence")
        || t.ends_with("ance")
    {
        return t.to_string();
    }

    let mut s = t.to_string();
    if s.len() > 6 && s.ends_with("ing") {
        // Only strip -ing for longer verbs (walking, leaving).
        let base = &s[..s.len() - 3];
        if base.len() >= 4 {
            s = base.to_string();
            let chars: Vec<char> = s.chars().collect();
            if chars.len() >= 2 && chars[chars.len() - 1] == chars[chars.len() - 2] {
                s.pop();
            }
        }
    } else if s.len() > 5 && s.ends_with("ies") {
        s.truncate(s.len() - 3);
        s.push('y');
    } else if s.len() > 5 && s.ends_with("es") && !s.ends_with("ness") && !s.ends_with("ous") {
        // churches → church; not produces (handled above) / houses careful
        let base = &s[..s.len() - 2];
        if base.ends_with('s')
            || base.ends_with('x')
            || base.ends_with('z')
            || base.ends_with("ch")
            || base.ends_with("sh")
        {
            s = base.to_string();
        }
    } else if s.len() > 4 && s.ends_with('s') && !s.ends_with("ss") && !s.ends_with("us") {
        s.pop();
    } else if s.len() > 5 && s.ends_with("ed") {
        s.truncate(s.len() - 2);
        if s.ends_with('i') {
            s.pop();
            s.push('y');
        }
    } else if s.len() > 5 && s.ends_with("ly") {
        s.truncate(s.len() - 2);
    }
    s
}

/// Select content stems for broad semantic expansion. Recall scaffolding is
/// removed, and contrastive phrasing expands only the asserted side so the
/// rejected alternative cannot dominate topic/neighbor retrieval.
pub fn semantic_query_stems(query: &NormalizedQuery) -> Vec<String> {
    let raw = query.raw.to_lowercase();
    for marker in [" rather than ", " instead of "] {
        if let Some(index) = raw.find(marker) {
            let positive = if index > 0 {
                &raw[..index]
            } else if let Some(comma) = raw[index + marker.len()..].find(',') {
                &raw[index + marker.len() + comma + 1..]
            } else {
                continue;
            };
            let mut normalized = normalize_query(positive);
            remove_recall_scaffolding(&mut normalized.lemmas);
            if !normalized.lemmas.is_empty() {
                return normalized.lemmas;
            }
        }
    }

    if !raw.contains("not only") {
        if let Some(index) = raw.find(" but ") {
            let before = &raw[..index];
            if before.split_whitespace().any(|token| token == "not") {
                let mut normalized = normalize_query(&raw[index + " but ".len()..]);
                remove_recall_scaffolding(&mut normalized.lemmas);
                if !normalized.lemmas.is_empty() {
                    return normalized.lemmas;
                }
            }
        }
    }

    let mut stems = query.lemmas.clone();
    remove_recall_scaffolding(&mut stems);
    stems
}

fn remove_recall_scaffolding(stems: &mut Vec<String>) {
    const SCAFFOLDING: &[&str] = &[
        "line",
        "says",
        "saying",
        "passage",
        "verse",
        "scripture",
        "bible",
        "quote",
        "wording",
        "remember",
    ];
    stems.retain(|stem| !SCAFFOLDING.contains(&stem.as_str()));
}

fn push_term(terms: &mut Vec<String>, t: &str) {
    if t.is_empty() || t.len() == 1 {
        return;
    }
    if STOP.contains(&t) {
        return;
    }
    // Avoid duplicates while preserving order.
    if !terms.iter().any(|x| x == t) {
        terms.push(t.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_stops_and_lowercases() {
        let q = normalize_query("  The one about the Lost Sheep!  ");
        // "one" and "about" and "the" are stops; lost+sheep remain.
        assert!(q.terms.contains(&"lost".to_string()));
        assert!(q.terms.contains(&"sheep".to_string()));
        assert_eq!(stem("lost"), "lose");
        assert_eq!(stem("loving"), "love");
        assert_eq!(stem("giving"), "give");
        assert_eq!(stem("gave"), "give");
        assert_eq!(stem("faithfulness"), "faithful");
        assert_eq!(stem("angry"), "anger");
        // Semantic synonymy belongs in the versioned expansion artifact, not
        // in morphology. These words must remain independently observable.
        assert_eq!(stem("afraid"), "afraid");
        assert_eq!(stem("scared"), "scare");
        assert_eq!(stem("start"), "start");
    }

    #[test]
    fn normalizer_v2_conformance_fixture_matches_authoritative_runtime() {
        let fixtures: Vec<[String; 2]> =
            serde_json::from_str(include_str!("../../../tests/normalizer-v2-fixtures.json"))
                .expect("normalizer v2 fixture must be valid JSON");
        assert!(!fixtures.is_empty());
        for [surface, expected] in fixtures {
            assert_eq!(stem(&surface), expected, "surface {surface:?}");
        }
    }

    #[test]
    fn situation_query_keeps_content_stems() {
        let q = normalize_query("verse about God loving the whole world and giving his son");
        assert!(q.lemmas.iter().any(|t| t == "love"));
        assert!(q.lemmas.iter().any(|t| t == "world"));
        assert!(q.lemmas.iter().any(|t| t == "give"));
        assert!(q.lemmas.iter().any(|t| t == "son"));
        assert!(q.lemmas.iter().any(|t| t == "god"));
    }

    #[test]
    fn semantic_stems_keep_asserted_side_of_a_contrast() {
        let q = normalize_query("which passage says to depend on God rather than my own judgment");
        assert_eq!(semantic_query_stems(&q), ["depend", "god"]);
    }
}
