//! Query-only planning for compound semantic searches.
//!
//! The planner deliberately has no access to packs, verses, embeddings, or
//! evaluation data. It only recognizes conservative grammatical boundaries
//! and refuses to split unless every resulting clause is independently useful.

use serde::Serialize;
use std::cmp::Ordering;

/// Stable identity written alongside host-computed segment embeddings.
///
/// Version two preserves source order and records grammatical boundary
/// relations internally so composition can distinguish an independent memory
/// from an alternative or subordinate view of the same memory.  The public
/// JSON shape remains `{ planner, segments }` for host compatibility.
pub const QUERY_SEGMENT_PLANNER: &str = "verse-recall-clause-v2";

/// Maximum number of clause embeddings accepted by the portable runtime.
pub const MAX_QUERY_SEGMENTS: usize = 4;

const MAX_BOUNDARIES: usize = 24;

/// Serializable result from the query-only clause planner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QuerySegmentPlan {
    pub planner: &'static str,
    pub segments: Vec<String>,
}

impl QuerySegmentPlan {
    fn new(segments: Vec<String>) -> Self {
        Self {
            planner: QUERY_SEGMENT_PLANNER,
            segments,
        }
    }

    /// Whether the query was conservatively decomposed into multiple intents.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoundaryRelation {
    /// Sentence punctuation or a dash between independently useful spans.
    Strong,
    /// Additive coordination such as “and”, “and then”, or “plus”.
    Additive,
    /// Competing recollections such as “or”; alternatives are one intent group.
    Alternative,
    /// Contrastive coordination such as “but”, “yet”, or “do not”.
    Contrast,
    /// Explanatory or temporal subordination such as “because” or “while”.
    Subordinate,
}

impl BoundaryRelation {
    /// Relations that may justify distinct lexical-only representatives.
    /// This remains part of the public v2 split-selection contract shared with
    /// the Python builder. Runtime composition classifies Contrast as a soft
    /// refinement even though it remains useful evidence for selecting a
    /// clause boundary here.
    pub(crate) const fn supports_independent_intent(self) -> bool {
        matches!(self, Self::Strong | Self::Additive | Self::Contrast)
    }
}

#[derive(Clone, Copy, Debug)]
struct Boundary {
    start: usize,
    /// Where the following segment starts. Usually `end`; contrast markers
    /// such as “do not” remain part of the right-hand segment.
    next_start: usize,
    relation: BoundaryRelation,
}

#[derive(Debug, Eq, PartialEq)]
struct CandidateScore {
    strong_count: usize,
    independent_count: usize,
    segment_count: usize,
    meaningful_total: usize,
    imbalance: usize,
    boundary_starts: Vec<usize>,
}

impl Ord for CandidateScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.strong_count
            .cmp(&other.strong_count)
            .then_with(|| self.independent_count.cmp(&other.independent_count))
            .then_with(|| self.segment_count.cmp(&other.segment_count))
            .then_with(|| self.meaningful_total.cmp(&other.meaningful_total))
            // Python scores `-imbalance`, so a smaller imbalance is better.
            .then_with(|| other.imbalance.cmp(&self.imbalance))
            // Python scores a tuple of negative starts. Earlier boundaries
            // therefore win lexicographically when all other evidence ties.
            .then_with(|| compare_earlier_boundaries(&self.boundary_starts, &other.boundary_starts))
    }
}

impl PartialOrd for CandidateScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Conservatively split a query into two to four independently meaningful
/// clauses. An empty `segments` list means the whole query should be embedded
/// and searched normally.
///
/// This is the runtime counterpart of `plan_query_segments` in
/// `scripts/build_semantic_vectors.py`. Both implementations share the stable
/// planner identity above and are kept under matching conformance tests.
pub fn plan_query_segments(query: &str) -> QuerySegmentPlan {
    let plan = plan_query_composition(query);
    QuerySegmentPlan::new(plan.segments)
}

/// Internal ordered plan used by lexical and embedded composition. Hosts only
/// need the public clause strings; the core recomputes these relations from the
/// original query before accepting or composing supplied vectors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueryCompositionPlan {
    pub(crate) segments: Vec<String>,
    /// Relation between segment `i` and `i + 1`.
    pub(crate) relations: Vec<BoundaryRelation>,
}

pub(crate) fn plan_query_composition(query: &str) -> QueryCompositionPlan {
    let query = collapse_whitespace(query);
    if query.is_empty() {
        return QueryCompositionPlan {
            segments: Vec::new(),
            relations: Vec::new(),
        };
    }

    let Some(boundaries) = collect_boundaries(&query) else {
        return QueryCompositionPlan {
            segments: Vec::new(),
            relations: Vec::new(),
        };
    };
    if boundaries.is_empty()
        || boundaries
            .iter()
            .filter(|boundary| boundary.relation == BoundaryRelation::Strong)
            .count()
            >= MAX_QUERY_SEGMENTS
    {
        return QueryCompositionPlan {
            segments: Vec::new(),
            relations: Vec::new(),
        };
    }

    let mut best: Option<(CandidateScore, QueryCompositionPlan)> = None;
    let max_selected = (MAX_QUERY_SEGMENTS - 1).min(boundaries.len());
    for selected_count in 1..=max_selected {
        visit_combinations(
            &query,
            &boundaries,
            selected_count,
            0,
            &mut Vec::with_capacity(selected_count),
            &mut best,
        );
    }

    best.map_or(
        QueryCompositionPlan {
            segments: Vec::new(),
            relations: Vec::new(),
        },
        |(_, plan)| plan,
    )
}

fn visit_combinations(
    query: &str,
    boundaries: &[Boundary],
    remaining: usize,
    next: usize,
    selected: &mut Vec<Boundary>,
    best: &mut Option<(CandidateScore, QueryCompositionPlan)>,
) {
    if remaining == 0 {
        consider_candidate(query, selected, best);
        return;
    }
    if boundaries.len().saturating_sub(next) < remaining {
        return;
    }
    for index in next..=boundaries.len() - remaining {
        selected.push(boundaries[index]);
        visit_combinations(query, boundaries, remaining - 1, index + 1, selected, best);
        selected.pop();
    }
}

fn consider_candidate(
    query: &str,
    selected: &[Boundary],
    best: &mut Option<(CandidateScore, QueryCompositionPlan)>,
) {
    let mut segments = Vec::with_capacity(selected.len() + 1);
    let mut start = 0;
    for boundary in selected {
        segments.push(clean_segment(&query[start..boundary.start]));
        start = boundary.next_start;
    }
    segments.push(clean_segment(&query[start..]));

    let counts: Vec<usize> = segments
        .iter()
        .map(|segment| meaningful_count(segment))
        .collect();
    if segments
        .iter()
        .zip(&counts)
        .any(|(segment, meaningful)| !independently_useful_segment(segment, *meaningful))
    {
        return;
    }

    let minimum = counts.iter().copied().min().unwrap_or(0);
    let maximum = counts.iter().copied().max().unwrap_or(0);
    let score = CandidateScore {
        strong_count: selected
            .iter()
            .filter(|boundary| boundary.relation == BoundaryRelation::Strong)
            .count(),
        independent_count: selected
            .iter()
            .filter(|boundary| boundary.relation.supports_independent_intent())
            .count(),
        segment_count: segments.len(),
        meaningful_total: counts.iter().sum(),
        imbalance: maximum - minimum,
        boundary_starts: selected.iter().map(|boundary| boundary.start).collect(),
    };
    let should_replace = match best.as_ref() {
        Some((current_score, _)) => score > *current_score,
        None => true,
    };
    if should_replace {
        *best = Some((
            score,
            QueryCompositionPlan {
                segments,
                relations: selected.iter().map(|boundary| boundary.relation).collect(),
            },
        ));
    }
}

fn collect_boundaries(query: &str) -> Option<Vec<Boundary>> {
    let mut boundaries = Vec::new();
    let mut cursor = 0;
    while cursor < query.len() {
        let Some((end, next_start, relation)) = boundary_at(query, cursor) else {
            cursor += query[cursor..]
                .chars()
                .next()
                .expect("valid cursor")
                .len_utf8();
            continue;
        };

        let is_abbreviation = relation == BoundaryRelation::Strong
            && &query[cursor..end] == "."
            && is_common_abbreviation(&query[..end]);
        if !is_abbreviation
            && !clean_segment(&query[..cursor]).is_empty()
            && !clean_segment(&query[next_start..]).is_empty()
        {
            boundaries.push(Boundary {
                start: cursor,
                next_start,
                relation,
            });
            if boundaries.len() > MAX_BOUNDARIES {
                return None;
            }
        }
        cursor = end;
    }
    Some(boundaries)
}

fn boundary_at(query: &str, start: usize) -> Option<(usize, usize, BoundaryRelation)> {
    let ch = query[start..].chars().next()?;
    match ch {
        ';' | '!' | '?' => {
            let mut end = start;
            for next in query[start..].chars() {
                if !matches!(next, ';' | '!' | '?') {
                    break;
                }
                end += next.len_utf8();
            }
            return Some((end, end, BoundaryRelation::Strong));
        }
        '.' => {
            let end = start + ch.len_utf8();
            let followed_by_boundary = match query[end..].chars().next() {
                Some(next) => next.is_whitespace(),
                None => true,
            };
            if followed_by_boundary {
                return Some((end, end, BoundaryRelation::Strong));
            }
            return None;
        }
        '\u{2013}' | '\u{2014}' => {
            let end = start + ch.len_utf8();
            return Some((end, end, BoundaryRelation::Strong));
        }
        _ => {}
    }

    if query[..start]
        .chars()
        .next_back()
        .is_some_and(is_word_character)
    {
        return None;
    }
    const COORDINATORS: &[(&[&str], BoundaryRelation, bool)] = &[
        (&["as", "well", "as"], BoundaryRelation::Additive, false),
        (&["and", "then"], BoundaryRelation::Additive, false),
        (&["do", "not"], BoundaryRelation::Contrast, true),
        (&["and"], BoundaryRelation::Additive, false),
        (&["but"], BoundaryRelation::Contrast, false),
        (&["or"], BoundaryRelation::Alternative, false),
        (&["yet"], BoundaryRelation::Contrast, false),
        (&["whereas"], BoundaryRelation::Contrast, false),
        (&["because"], BoundaryRelation::Subordinate, false),
        (&["while"], BoundaryRelation::Subordinate, false),
        (&["plus"], BoundaryRelation::Additive, false),
    ];
    for (words, relation, retain_right) in COORDINATORS {
        let Some(end) = match_ascii_words(query, start, words) else {
            continue;
        };
        if !query[end..].chars().next().is_some_and(is_word_character) {
            return Some((end, if *retain_right { start } else { end }, *relation));
        }
    }
    None
}

fn match_ascii_words(query: &str, start: usize, words: &[&str]) -> Option<usize> {
    let mut cursor = start;
    for (index, word) in words.iter().enumerate() {
        let end = cursor.checked_add(word.len())?;
        if !query.get(cursor..end)?.eq_ignore_ascii_case(word) {
            return None;
        }
        cursor = end;
        if index + 1 < words.len() {
            let mut whitespace = 0;
            while let Some(ch) = query[cursor..].chars().next() {
                if !ch.is_whitespace() {
                    break;
                }
                whitespace += 1;
                cursor += ch.len_utf8();
            }
            if whitespace == 0 {
                return None;
            }
        }
    }
    Some(cursor)
}

fn is_common_abbreviation(prefix: &str) -> bool {
    const ABBREVIATIONS: &[&str] = &[
        "mr.", "mrs.", "ms.", "dr.", "st.", "rev.", "vs.", "cf.", "etc.", "no.", "e.g.", "i.e.",
    ];
    ABBREVIATIONS.iter().any(|abbreviation| {
        if prefix.len() < abbreviation.len() {
            return false;
        }
        let start = prefix.len() - abbreviation.len();
        prefix
            .get(start..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(abbreviation))
            && !prefix[..start]
                .chars()
                .next_back()
                .is_some_and(is_word_character)
    })
}

fn collapse_whitespace(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending_space = !output.is_empty();
        } else {
            if pending_space {
                output.push(' ');
                pending_space = false;
            }
            output.push(ch);
        }
    }
    output
}

fn clean_segment(text: &str) -> String {
    collapse_whitespace(text)
        .trim_matches(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ',' | ';' | ':' | '.' | '!' | '?' | '\u{2013}' | '\u{2014}'
                )
        })
        .to_string()
}

fn query_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut characters = text.chars().peekable();
    while let Some(ch) = characters.next() {
        let joins_word = matches!(ch, '\'' | '\u{2019}')
            && !current.is_empty()
            && characters.peek().is_some_and(|next| next.is_alphanumeric());
        if ch.is_alphanumeric() || joins_word {
            current.push(ch);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn meaningful_count(text: &str) -> usize {
    query_words(text)
        .iter()
        .filter(|word| {
            word.chars().any(char::is_alphabetic) && !is_function_word(&word.to_ascii_lowercase())
        })
        .count()
}

fn independently_useful_segment(text: &str, meaningful: usize) -> bool {
    let words = query_words(text);
    if words.len() < 2 || meaningful == 0 {
        return false;
    }
    if meaningful >= 2 {
        return true;
    }
    // A determiner plus one content noun is a complete noun phrase rather than
    // a dangling grammatical fragment. This admits generic concept/event
    // clauses while still refusing fragments such as “not by sight”.
    words
        .first()
        .is_some_and(|word| matches!(word.to_ascii_lowercase().as_str(), "a" | "an" | "the"))
}

fn is_word_character(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn is_function_word(word: &str) -> bool {
    matches!(
        word,
        "a" | "about"
            | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "been"
            | "being"
            | "but"
            | "by"
            | "did"
            | "do"
            | "does"
            | "for"
            | "from"
            | "had"
            | "has"
            | "have"
            | "he"
            | "her"
            | "hers"
            | "him"
            | "his"
            | "how"
            | "i"
            | "if"
            | "in"
            | "into"
            | "is"
            | "it"
            | "its"
            | "me"
            | "my"
            | "nor"
            | "not"
            | "of"
            | "on"
            | "or"
            | "our"
            | "ours"
            | "she"
            | "so"
            | "than"
            | "that"
            | "the"
            | "their"
            | "theirs"
            | "them"
            | "then"
            | "there"
            | "these"
            | "they"
            | "this"
            | "those"
            | "to"
            | "us"
            | "was"
            | "we"
            | "were"
            | "what"
            | "when"
            | "where"
            | "which"
            | "who"
            | "why"
            | "will"
            | "with"
            | "would"
            | "you"
            | "your"
            | "yours"
    )
}

fn compare_earlier_boundaries(left: &[usize], right: &[usize]) -> Ordering {
    for (left, right) in left.iter().zip(right) {
        match right.cmp(left) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    left.len().cmp(&right.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(query: &str) -> Vec<String> {
        plan_query_segments(query).segments
    }

    #[test]
    fn matches_python_planner_conformance_examples() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/query_segment_planner_cases.json"
        ))
        .expect("shared planner conformance cases");
        for case in cases.as_array().expect("case array") {
            let query = case["query"].as_str().expect("query");
            let expected: Vec<String> = case["segments"]
                .as_array()
                .expect("segments")
                .iter()
                .map(|segment| segment.as_str().expect("segment").to_string())
                .collect();
            assert_eq!(segments(query), expected, "{query}");
        }
    }

    #[test]
    fn unicode_offsets_apostrophes_and_whitespace_are_safe() {
        assert_eq!(
            segments("Élie prie — Jésus nourrit la foule"),
            ["Élie prie", "Jésus nourrit la foule"]
        );
        assert_eq!(
            segments("Ἰησοῦς διδάσκει — l’amour d’autrui demeure"),
            ["Ἰησοῦς διδάσκει", "l’amour d’autrui demeure"]
        );
        assert_eq!(
            segments("  first\t meaningful thought   AND THEN\nsecond meaningful memory  "),
            ["first meaningful thought", "second meaningful memory"]
        );
    }

    #[test]
    fn refuses_fragments_abbreviations_and_unrepresentable_paragraphs() {
        for query in [
            "faith and hope",
            "walk by faith and not by sight",
            "the fall",
            "one image; two picture; three memory; four vision; five detail",
        ] {
            assert!(segments(query).is_empty(), "{query}");
        }
        assert_eq!(
            segments(
                "Mary remembers Dr. Luke writing carefully and Paul teaches the gathered church"
            ),
            [
                "Mary remembers Dr. Luke writing carefully",
                "Paul teaches the gathered church"
            ]
        );

        let pasted = (0..26)
            .map(|index| format!("meaningful image {index}"))
            .collect::<Vec<_>>()
            .join(" and ");
        assert!(segments(&pasted).is_empty());
    }

    #[test]
    fn plan_has_stable_public_identity() {
        let plan = plan_query_segments("lost sheep and prodigal son");
        assert_eq!(plan.planner, QUERY_SEGMENT_PLANNER);
        assert!(!plan.is_empty());
        assert_eq!(
            serde_json::to_string(&plan).unwrap(),
            r#"{"planner":"verse-recall-clause-v2","segments":["lost sheep","prodigal son"]}"#
        );
    }

    #[test]
    fn composition_plan_classifies_boundaries_and_preserves_order() {
        let additive =
            plan_query_composition("first meaningful scene and second meaningful picture");
        assert_eq!(
            additive.segments,
            ["first meaningful scene", "second meaningful picture"]
        );
        assert_eq!(additive.relations, [BoundaryRelation::Additive]);

        let alternative =
            plan_query_composition("first remembered location or second remembered location");
        assert_eq!(alternative.relations, [BoundaryRelation::Alternative]);

        let subordinate =
            plan_query_composition("rain nourishes a field because another image describes a tree");
        assert_eq!(subordinate.relations, [BoundaryRelation::Subordinate]);

        let contrast =
            plan_query_composition("stand firmly with courage do not fear approaching danger");
        assert_eq!(
            contrast.segments,
            [
                "stand firmly with courage",
                "do not fear approaching danger"
            ]
        );
        assert_eq!(contrast.relations, [BoundaryRelation::Contrast]);

        let strong = plan_query_composition("first remembered picture; second remembered picture");
        assert_eq!(strong.relations, [BoundaryRelation::Strong]);
    }
}
