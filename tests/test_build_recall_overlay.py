from __future__ import annotations

import importlib.util
import json
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts" / "build_recall_overlay.py"
SPEC = importlib.util.spec_from_file_location("build_recall_overlay", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
overlay = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = overlay
SPEC.loader.exec_module(overlay)


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.write_text(
        "".join(json.dumps(row, ensure_ascii=False) + "\n" for row in rows),
        encoding="utf-8",
    )


def write_json(path: Path, value: object) -> None:
    path.write_text(
        json.dumps(value, ensure_ascii=False),
        encoding="utf-8",
    )


class BuildRecallOverlayTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.canonical = self.root / "bsb.jsonl"
        write_jsonl(
            self.canonical,
            [
                {
                    "ref": "Gen.1.1",
                    "book": "Genesis",
                    "chapter": "1",
                    "verseNum": "1",
                    "text": "Canonical one",
                    "events": ["Creation"],
                },
                {
                    "ref": "Gen.1.2",
                    "book": "Genesis",
                    "chapter": "1",
                    "verseNum": "2",
                    "text": "Canonical two",
                    "events": ["Creation"],
                },
                {
                    "ref": "Gen.2.1",
                    "book": "Genesis",
                    "chapter": "2",
                    "verseNum": "1",
                    "text": "Canonical three",
                    "events": [],
                },
                {
                    "ref": "Gen.3.1",
                    "book": "Genesis",
                    "chapter": "3",
                    "verseNum": "1",
                    "text": "Canonical four",
                    "events": ["The Fall"],
                },
                {
                    "ref": "Gen.3.2",
                    "book": "Genesis",
                    "chapter": "3",
                    "verseNum": "2",
                    "text": "Canonical five",
                    "events": ["The Fall"],
                },
            ],
        )

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def test_groups_events_and_aligns_alternate_text(self) -> None:
        alternate = self.root / "kjv.jsonl"
        write_jsonl(
            alternate,
            [
                {
                    "ref": "Gen.1.1",
                    "book": "Genesis",
                    "chapter": 1,
                    "verseNum": 1,
                    "text": "Alternate one",
                },
                {
                    "ref": "Gen.1.2",
                    "book": "Genesis",
                    "chapter": 1,
                    "verseNum": 2,
                    "text": "Alternate two",
                },
                {
                    "ref": "Gen.2.1",
                    "book": "Genesis",
                    "chapter": 2,
                    "verseNum": 1,
                    "text": "Alternate three",
                },
                {
                    "ref": "Gen.3.1",
                    "book": "Genesis",
                    "chapter": 3,
                    "verseNum": 1,
                    "text": "Alternate four",
                },
                {
                    "book": "Genesis",
                    "chapter": 3,
                    "verseNum": 2,
                    "text": "Alternate five",
                },
                {
                    "ref": "John.5.4",
                    "book": "John",
                    "chapter": 5,
                    "verseNum": 4,
                    "text": "Source-only verse",
                },
            ],
        )
        output = self.root / "overlay.jsonl"
        manifest = overlay.write_overlay(
            canonical_path=self.canonical,
            output_path=output,
            alternates=[overlay.AlternateSource("KJV", alternate)],
        )

        rows = [json.loads(line) for line in output.read_text().splitlines()]
        self.assertEqual("manifest", rows[0]["kind"])
        self.assertEqual(2, manifest["concept_sources"][0]["record_count"])
        self.assertEqual(
            0, manifest["concept_sources"][0]["quarantined_event_count"]
        )
        self.assertEqual(5, manifest["alternate_sources"][0]["matched_verse_count"])
        self.assertEqual(
            1, manifest["alternate_sources"][0]["unmatched_source_verse_count"]
        )

        fall = next(row for row in rows if row.get("key") == "the fall")
        self.assertEqual("Genesis 3:1", fall["anchor_ref"])
        self.assertEqual(
            [
                {
                    "start_ref": "Genesis 3:1",
                    "end_ref": "Genesis 3:2",
                    "verse_count": 2,
                }
            ],
            fall["target_ranges"],
        )
        alternate_rows = [row for row in rows if row["kind"] == "alternate_text"]
        self.assertEqual(
            [
                "Genesis 1:1",
                "Genesis 1:2",
                "Genesis 2:1",
                "Genesis 3:1",
                "Genesis 3:2",
            ],
            [row["target_ref"] for row in alternate_rows],
        )

    def test_imports_study_bible_graph_as_comprehensive_shadow_evidence(self) -> None:
        database = self.root / "study_bible.db"
        connection = sqlite3.connect(database)
        connection.executescript(
            """
            CREATE TABLE graph_people (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, also_called TEXT
            );
            CREATE TABLE graph_places (id TEXT PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE graph_events (id TEXT PRIMARY KEY, title TEXT NOT NULL);
            CREATE TABLE graph_verse_mentions (
                verse_ref TEXT NOT NULL, entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL
            );
            INSERT INTO graph_people VALUES ('abraham_1', 'Abraham', 'Abram');
            INSERT INTO graph_people VALUES ('unmentioned_2', 'Unmentioned', NULL);
            INSERT INTO graph_places VALUES ('eden_1', 'Eden');
            INSERT INTO graph_events VALUES ('3', 'The Fall');
            INSERT INTO graph_verse_mentions VALUES ('Gen.1.1', 'person', 'abraham_1');
            INSERT INTO graph_verse_mentions VALUES ('Gen.1.2', 'person', 'abraham_1');
            INSERT INTO graph_verse_mentions VALUES ('Gen.3.1', 'place', 'eden_1');
            INSERT INTO graph_verse_mentions VALUES ('Gen.3.1', 'event', '3');
            INSERT INTO graph_verse_mentions VALUES ('Gen.3.2', 'event', '3');
            INSERT INTO graph_verse_mentions VALUES ('Exod.1.1', 'person', 'abraham_1');
            INSERT INTO graph_verse_mentions VALUES ('Gen.1.1', 'person', 'missing');
            """
        )
        connection.commit()
        connection.close()

        output = self.root / "study-overlay.jsonl"
        manifest = overlay.write_overlay(
            canonical_path=self.canonical,
            output_path=output,
            alternates=[],
            include_events=False,
            study_bible_db_path=database,
        )
        rows = [json.loads(line) for line in output.read_text().splitlines()]
        concepts = [row for row in rows if row["kind"] == "concept"]
        self.assertEqual(
            ["person", "place", "event_shadow"],
            [row["concept_type"] for row in concepts],
        )
        abraham = concepts[0]
        self.assertEqual(["Abram"], abraham["aliases"])
        self.assertEqual(
            [{
                "start_ref": "Genesis 1:1",
                "end_ref": "Genesis 1:2",
                "verse_count": 2,
            }],
            abraham["target_ranges"],
        )
        self.assertEqual("CC BY-SA 4.0", manifest["concept_sources"][0]["source_license"])
        self.assertEqual(
            1,
            manifest["concept_sources"][0]["entity_without_canonical_target_count"],
        )
        event_source = manifest["concept_sources"][2]
        self.assertEqual(1, event_source["invalid_or_missing_canonical_ref_count"])
        self.assertEqual(1, event_source["unknown_entity_mention_count"])

    def test_study_bible_graph_rejects_duplicate_entity_ids(self) -> None:
        database = self.root / "malformed-study.db"
        connection = sqlite3.connect(database)
        connection.executescript(
            """
            CREATE TABLE graph_people (id TEXT, name TEXT, also_called TEXT);
            CREATE TABLE graph_places (id TEXT, name TEXT);
            CREATE TABLE graph_events (id TEXT, title TEXT);
            CREATE TABLE graph_verse_mentions (
                verse_ref TEXT, entity_type TEXT, entity_id TEXT
            );
            INSERT INTO graph_people VALUES ('same', 'First', NULL);
            INSERT INTO graph_people VALUES ('same', 'Second', NULL);
            """
        )
        connection.commit()
        connection.close()

        with self.assertRaisesRegex(ValueError, "repeats id 'same'"):
            overlay.build_study_bible_records(
                database, overlay.load_canonical(self.canonical)
            )

    def test_study_bible_graph_rejects_malformed_known_mentions(self) -> None:
        database = self.root / "malformed-mention.db"
        connection = sqlite3.connect(database)
        connection.executescript(
            """
            CREATE TABLE graph_people (id TEXT, name TEXT, also_called TEXT);
            CREATE TABLE graph_places (id TEXT, name TEXT);
            CREATE TABLE graph_events (id TEXT, title TEXT);
            CREATE TABLE graph_verse_mentions (
                verse_ref TEXT, entity_type TEXT, entity_id TEXT
            );
            INSERT INTO graph_people VALUES ('person', 'Person', NULL);
            INSERT INTO graph_verse_mentions VALUES (NULL, 'person', 'person');
            """
        )
        connection.commit()
        connection.close()

        with self.assertRaisesRegex(ValueError, "invalid verse_ref"):
            overlay.build_study_bible_records(
                database, overlay.load_canonical(self.canonical)
            )

    def test_output_is_byte_deterministic_regardless_of_source_order(self) -> None:
        first = self.root / "first.jsonl"
        second = self.root / "second.jsonl"
        for path, label in ((first, "first"), (second, "second")):
            write_jsonl(
                path,
                [
                    {
                        "ref": f"Gen.{chapter}.{verse}",
                        "book": "Genesis",
                        "chapter": chapter,
                        "verseNum": verse,
                        "text": f"{label} {chapter}:{verse}",
                    }
                    for chapter, verse in ((1, 1), (1, 2), (2, 1), (3, 1), (3, 2))
                ],
            )
        out_a = self.root / "a.jsonl"
        out_b = self.root / "b.jsonl"
        sources = [
            overlay.AlternateSource("ZZZ", first),
            overlay.AlternateSource("AAA", second),
        ]
        overlay.write_overlay(
            canonical_path=self.canonical,
            output_path=out_a,
            alternates=sources,
        )
        overlay.write_overlay(
            canonical_path=self.canonical,
            output_path=out_b,
            alternates=reversed(sources),
        )
        self.assertEqual(out_a.read_bytes(), out_b.read_bytes())

    def test_low_coverage_fails_without_replacing_existing_output(self) -> None:
        alternate = self.root / "short.jsonl"
        write_jsonl(
            alternate,
            [
                {
                    "ref": "Gen.1.1",
                    "book": "Genesis",
                    "chapter": 1,
                    "verseNum": 1,
                    "text": "Only one",
                }
            ],
        )
        output = self.root / "overlay.jsonl"
        output.write_text("keep me", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "coverage"):
            overlay.write_overlay(
                canonical_path=self.canonical,
                output_path=output,
                alternates=[overlay.AlternateSource("SHORT", alternate)],
                min_coverage=0.99,
            )
        self.assertEqual("keep me", output.read_text(encoding="utf-8"))

    def test_alternate_ref_and_coordinates_must_agree(self) -> None:
        alternate = self.root / "conflicting.jsonl"
        write_jsonl(
            alternate,
            [
                {
                    "ref": "Gen.1.1",
                    "book": "Genesis",
                    "chapter": 3,
                    "verseNum": 1,
                    "text": "Coordinates point at a different verse",
                }
            ],
        )
        with self.assertRaisesRegex(ValueError, "conflicts with ref"):
            overlay.align_alternate(
                overlay.AlternateSource("BAD", alternate),
                overlay.load_canonical(self.canonical),
            )

    def test_topics_map_exact_refs_keep_typed_event_precedence_and_report_skips(
        self,
    ) -> None:
        canonical = self.root / "topic-bsb.jsonl"
        canonical_rows: list[dict] = []

        def add_passage(
            book: str,
            ref_book: str,
            chapter: int,
            verses: range,
            events: list[str] | None = None,
        ) -> None:
            for verse in verses:
                canonical_rows.append(
                    {
                        "ref": f"{ref_book}.{chapter}.{verse}",
                        "book": book,
                        "chapter": chapter,
                        "verseNum": verse,
                        "text": f"{book} {chapter}:{verse}",
                        "events": events or [],
                    }
                )

        add_passage("Genesis", "Gen", 3, range(1, 3), ["The Fall"])
        add_passage(
            "Matthew", "Matt", 5, range(1, 4), ["Sermon on the Mount"]
        )
        add_passage("Luke", "Luke", 10, range(25, 38))
        add_passage("Romans", "Rom", 5, range(12, 13))
        write_jsonl(canonical, canonical_rows)

        topic_rows = [
            ["unmapped", "unmapped topic"],
            ["the_fall", "the fall"],
            ["good_samaritan", "good samaritan"],
            ["no_ranking", "no ranking"],
            ["sermon_on_the_mount", "sermon on the mount"],
        ]
        ranking_rows = [
            ["orphan", [["GEN.3.1", 99, 1]]],
            ["unmapped", [["JHN.5.4", 2, 1]]],
            ["good_samaritan", [["LUK.10.25-37", 5, 1]]],
            ["the_fall", [["ROM.5.12", 8, 1]]],
            ["sermon_on_the_mount", [["MAT.5.1-3", 5, 1]]],
        ]
        topics = self.root / "canonical-topics.browser.json"
        rankings = self.root / "topic-verse-rankings.browser.json"
        write_json(topics, topic_rows)
        write_json(rankings, ranking_rows)

        output = self.root / "topic-overlay.jsonl"
        manifest = overlay.write_overlay(
            canonical_path=canonical,
            output_path=output,
            alternates=[],
            topic_sources=overlay.TopicSources(topics, rankings),
        )
        rows = [json.loads(line) for line in output.read_text().splitlines()]
        concepts = [row for row in rows if row["kind"] == "concept"]
        event_fall = next(
            row
            for row in concepts
            if row["concept_type"] == "event" and row["key"] == "the fall"
        )
        topic_fall = next(
            row
            for row in concepts
            if row["concept_type"] == "topic" and row["key"] == "the fall"
        )
        self.assertLess(concepts.index(event_fall), concepts.index(topic_fall))
        self.assertEqual("Genesis 3:1", event_fall["anchor_ref"])
        self.assertEqual("Romans 5:12", topic_fall["anchor_ref"])

        samaritan = next(row for row in concepts if row["key"] == "good samaritan")
        self.assertEqual(["the good samaritan"], samaritan["aliases"])
        self.assertEqual(
            [
                {
                    "start_ref": "Luke 10:25",
                    "end_ref": "Luke 10:37",
                    "verse_count": 13,
                }
            ],
            samaritan["target_ranges"],
        )
        sermon_topic = next(
            row
            for row in concepts
            if row["concept_type"] == "topic"
            and row["key"] == "sermon on the mount"
        )
        self.assertEqual(["the sermon on the mount"], sermon_topic["aliases"])

        topic_source = manifest["concept_sources"][1]
        self.assertEqual(3, topic_source["record_count"])
        self.assertEqual(2, topic_source["skipped_topic_count"])
        self.assertEqual(2, topic_source["generated_alias_count"])
        self.assertEqual(2, topic_source["event_overlap_topic_count"])
        self.assertEqual(1, topic_source["missing_ranking_topic_count"])
        self.assertEqual(1, topic_source["unknown_ranking_topic_count"])
        self.assertEqual(1, topic_source["missing_canonical_target_count"])
        self.assertEqual(1, topic_source["topic_without_valid_target_count"])

        # Tuple input order is not semantic; ranks and normalized keys determine
        # output order and diagnostics.
        reversed_topics = self.root / "reversed-topics.json"
        reversed_rankings = self.root / "reversed-rankings.json"
        write_json(reversed_topics, list(reversed(topic_rows)))
        write_json(reversed_rankings, list(reversed(ranking_rows)))
        second_output = self.root / "topic-overlay-reversed.jsonl"
        overlay.write_overlay(
            canonical_path=canonical,
            output_path=second_output,
            alternates=[],
            topic_sources=overlay.TopicSources(
                reversed_topics, reversed_rankings
            ),
        )
        self.assertEqual(output.read_bytes(), second_output.read_bytes())

    def test_topic_tuple_schema_rejects_metadata_as_a_ranking_ref(self) -> None:
        topics = self.root / "canonical-topics.browser.json"
        rankings = self.root / "topic-verse-rankings.browser.json"
        write_json(topics, [["creation", "creation"]])
        write_json(
            rankings,
            [["creation", [["GEN.1.1", 7, 1, {"alias": "beginnings"}]]]],
        )
        output = self.root / "overlay.jsonl"
        output.write_text("keep me", encoding="utf-8")
        with self.assertRaisesRegex(
            ValueError, "expected \\[canonical_ref, raw_score, rank\\]"
        ):
            overlay.write_overlay(
                canonical_path=self.canonical,
                output_path=output,
                alternates=[],
                topic_sources=overlay.TopicSources(topics, rankings),
            )
        self.assertEqual("keep me", output.read_text(encoding="utf-8"))

    def test_event_annotations_choose_a_dominant_primary_and_quarantine_noise(
        self,
    ) -> None:
        canonical = self.root / "event-shapes.jsonl"
        rows: list[dict] = []

        def add(
            ref: str,
            book: str,
            chapter: int,
            verse: int,
            events: list[str],
            text: str | None = None,
        ) -> None:
            rows.append(
                {
                    "ref": ref,
                    "book": book,
                    "chapter": chapter,
                    "verseNum": verse,
                    "text": text or ref,
                    "events": events,
                }
            )

        add(
            "Gen.34.1",
            "Genesis",
            34,
            1,
            ["Death of Moses", "Lifetime of Moses"],
        )
        add("Gen.34.2", "Genesis", 34, 2, [])
        for verse in range(1, 13):
            add(
                f"Deut.34.{verse}",
                "Deuteronomy",
                34,
                verse,
                ["Death of Moses"],
                "Moses died there in Moab" if verse == 5 else None,
            )
        add(
            "Job.1.1",
            "Job",
            1,
            1,
            ["Lifetime of Moses", "Birth of Job"],
        )
        add("Job.1.2", "Job", 1, 2, [])
        for verse in range(1, 302):
            add(
                f"Exod.40.{verse}",
                "Exodus",
                40,
                verse,
                ["Tabernacle Built"],
            )
        add("Gen.3.1", "Genesis", 3, 1, ["The Fall"])
        add("Gen.3.2", "Genesis", 3, 2, ["The Fall"])
        for verse in range(24, 27):
            add(
                f"Gen.25.{verse}",
                "Genesis",
                25,
                verse,
                ["Lifetime of Jacob"],
            )
        add(
            "Gen.47.28",
            "Genesis",
            47,
            28,
            ["Death of Jacob"],
            "Jacob lived in Egypt for seventeen years",
        )
        for verse in range(28, 32):
            add(
                f"Gen.49.{verse}",
                "Genesis",
                49,
                verse,
                ["Death of Jacob"],
                "Jacob described where he wished to be buried",
            )
        for verse in range(16, 24):
            add(
                f"Judg.2.{verse}",
                "Judges",
                2,
                verse,
                ["Deliverance by Othniel"],
                "The judges delivered Israel",
            )
        for verse in range(9, 12):
            add(
                f"Judg.3.{verse}",
                "Judges",
                3,
                verse,
                ["Deliverance by Othniel"],
                "Othniel was raised up as a deliverer",
            )
        for verse in range(1, 4):
            add(
                f"Matt.5.{verse}",
                "Matthew",
                5,
                verse,
                ["Sermon on the Mount"],
            )
        write_jsonl(canonical, rows)

        output = self.root / "event-overlay.jsonl"
        manifest = overlay.write_overlay(
            canonical_path=canonical,
            output_path=output,
            alternates=[],
        )
        concepts = [
            json.loads(line)
            for line in output.read_text().splitlines()
            if json.loads(line).get("kind") == "concept"
        ]
        by_key = {row["key"]: row for row in concepts}
        self.assertEqual(
            {
                "death of moses",
                "deliverance by othniel",
                "sermon on the mount",
                "the fall",
            },
            set(by_key),
        )
        death = by_key["death of moses"]
        self.assertEqual("Deuteronomy 34:5", death["anchor_ref"])
        self.assertEqual(12, death["target_ranges"][0]["verse_count"])
        self.assertEqual(1, len(death["target_ranges"]))
        othniel = by_key["deliverance by othniel"]
        self.assertEqual("Judges 3:9", othniel["anchor_ref"])
        self.assertEqual("Judges 3:9", othniel["target_ranges"][0]["start_ref"])
        self.assertEqual(1, len(othniel["target_ranges"]))

        event_source = manifest["concept_sources"][0]
        self.assertEqual(9, event_source["input_event_count"])
        self.assertEqual(4, event_source["record_count"])
        self.assertEqual(5, event_source["quarantined_event_count"])
        quarantine = {
            row["key"]: row["reasons"]
            for row in event_source["quarantined_event_sample"]
        }
        self.assertEqual(
            ["primary_range_too_short", "missing_birth_cue"],
            quarantine["birth of job"],
        )
        self.assertEqual(
            [
                "unsupported_lifetime_summary",
                "primary_range_too_short",
                "fragmented_annotations",
            ],
            quarantine["lifetime of moses"],
        )
        self.assertEqual(
            ["unsupported_lifetime_summary"],
            quarantine["lifetime of jacob"],
        )
        self.assertEqual(["missing_death_cue"], quarantine["death of jacob"])
        self.assertEqual(
            ["primary_range_too_long"], quarantine["tabernacle built"]
        )

    def test_event_signature_stem_matches_runtime_normalizer_v2(self) -> None:
        fixture_path = SCRIPT.parents[1] / "tests" / "normalizer-v2-fixtures.json"
        fixtures = json.loads(fixture_path.read_text(encoding="utf-8"))
        self.assertEqual(2, overlay.EVENT_SIGNATURE_NORMALIZER_VERSION)
        for surface, expected in fixtures:
            with self.subTest(surface=surface):
                self.assertEqual(expected, overlay.event_signature_stem(surface))

    def test_event_signatures_are_corpus_unique_conservative_and_deterministic(
        self,
    ) -> None:
        def event_record(key: str, label: str, verse: int) -> dict:
            reference = f"Example 1:{verse}"
            return {
                "schema_version": 1,
                "kind": "concept",
                "concept_type": "event",
                "key": key,
                "label": label,
                "aliases": [],
                "source": "BSB.events",
                "anchor_ref": reference,
                "target_ranges": [
                    {
                        "start_ref": reference,
                        "end_ref": reference,
                        "verse_count": 1,
                    }
                ],
            }

        labels = {
            "artisan observes bioluminescence": (
                "Artisan observes bioluminescence",
            ),
            "artisan repairs cobalt lantern": (
                "Artisan repairs cobalt lantern",
            ),
            "sailor repairs cobalt lantern": (
                "Sailor repairs cobalt lantern",
            ),
            # This source event has no direct-safe record, but it still owns
            # signatures in the global collision registry.
            "scholar repairs cobalt lantern": (
                "Scholar repairs cobalt lantern",
            ),
            # This label cannot generate a partial candidate because all of
            # its source words are title-cased.  It must nevertheless own any
            # contained signature and make the shorter route ambiguous.
            "guild repairs cobalt lantern tonight": (
                "Guild Repairs Cobalt Lantern Tonight",
            ),
            "marisol charts zephyria": ("Marisol charts Zephyria",),
            # The lower-case spelling is singleton-eligible, but the same stem
            # occurs in Marisol's title-cased label and therefore is not
            # globally unique.
            "pilgrim enters zephyria": ("Pilgrim enters zephyria",),
            "reign of zephyria": ("Reign of Zephyria",),
        }
        records = (
            event_record(
                "artisan observes bioluminescence",
                "Artisan observes bioluminescence",
                1,
            ),
            event_record(
                "artisan repairs cobalt lantern",
                "Artisan repairs cobalt lantern",
                2,
            ),
            event_record(
                "sailor repairs cobalt lantern",
                "Sailor repairs cobalt lantern",
                3,
            ),
            event_record(
                "marisol charts zephyria", "Marisol charts Zephyria", 4
            ),
            event_record(
                "pilgrim enters zephyria", "Pilgrim enters zephyria", 5
            ),
            event_record("reign of zephyria", "Reign of Zephyria", 6),
        )
        primary_terms = {
            key: frozenset(overlay.event_signature_terms(label)[0])
            for key, (label,) in labels.items()
            if key
            not in {
                "scholar repairs cobalt lantern",
                "guild repairs cobalt lantern tonight",
            }
        }
        document_frequency: dict[str, int] = {}
        for terms in primary_terms.values():
            for stem in terms:
                document_frequency[stem] = document_frequency.get(stem, 0) + 1

        first, stats = overlay.attach_event_signature_aliases(
            records, labels, primary_terms, document_frequency
        )
        reversed_labels = dict(reversed(list(labels.items())))
        second, second_stats = overlay.attach_event_signature_aliases(
            records, reversed_labels, primary_terms, document_frequency
        )
        self.assertEqual(first, second)
        self.assertEqual(stats, second_stats)

        aliases = {
            record["key"]: record["aliases"] for record in first
        }
        observed = aliases["artisan observes bioluminescence"]
        self.assertTrue(any(":full:" in alias for alias in observed))
        self.assertTrue(any(":partial:" in alias for alias in observed))
        self.assertIn(
            "event-signature-v1:single:bioluminescence", observed
        )

        collision_surface = "event-signature-v1:partial:cobalt lantern repair"
        self.assertNotIn(
            collision_surface, aliases["artisan repairs cobalt lantern"]
        )
        self.assertNotIn(
            collision_surface, aliases["sailor repairs cobalt lantern"]
        )
        collision = next(
            sample
            for sample in stats["collision_samples"]
            if sample["signature"] == "cobalt lantern repair"
        )
        self.assertEqual(
            [
                "artisan repairs cobalt lantern",
                "guild repairs cobalt lantern tonight",
                "sailor repairs cobalt lantern",
                "scholar repairs cobalt lantern",
            ],
            collision["event_keys"],
        )

        # Title-cased person/place tokens never become singleton signatures.
        self.assertFalse(
            any(":single:" in alias for alias in aliases["marisol charts zephyria"])
        )
        self.assertFalse(
            any(":single:zephyria" in alias for alias in aliases["pilgrim enters zephyria"])
        )
        # Era summaries are counted but receive no generated route.
        self.assertEqual([], aliases["reign of zephyria"])
        self.assertEqual(1, stats["suppressed_unsafe_event_count"])
        self.assertGreater(stats["suppressed_unsafe_candidate_count"], 0)
        self.assertGreaterEqual(stats["corpus_collision_signature_count"], 1)
        self.assertGreaterEqual(stats["suppressed_collision_candidate_count"], 2)

    def test_manifest_reconciles_generated_event_signature_aliases(self) -> None:
        canonical = self.root / "signature-events.jsonl"
        rows = []
        for verse in range(1, 3):
            rows.append(
                {
                    "ref": f"Example.1.{verse}",
                    "book": "Genesis",
                    "chapter": 1,
                    "verseNum": verse,
                    "text": "An artisan walked through a silver labyrinth.",
                    "events": ["Artisan walks through silver labyrinth"],
                }
            )
        write_jsonl(canonical, rows)
        output = self.root / "signatures.jsonl"
        manifest = overlay.write_overlay(
            canonical_path=canonical,
            output_path=output,
            alternates=[],
        )
        source = manifest["concept_sources"][0]
        concepts = [
            json.loads(line)
            for line in output.read_text(encoding="utf-8").splitlines()[1:]
        ]
        actual_alias_count = sum(len(record["aliases"]) for record in concepts)
        actual_signature_count = sum(
            alias.startswith(overlay.EVENT_SIGNATURE_ALIAS_PREFIX)
            for record in concepts
            for alias in record["aliases"]
        )
        self.assertEqual(actual_alias_count, source["alias_count"])
        self.assertEqual(
            actual_signature_count, source["generated_signature_count"]
        )
        self.assertEqual(
            source["generated_signature_count"],
            source["full_signature_count"]
            + source["partial_signature_count"]
            + source["single_token_signature_count"],
        )

    def test_provenance_free_curated_concepts_are_rejected(self) -> None:
        curated = self.root / "curated.json"
        records = [
            {
                "key": "fall of man",
                "label": "Fall of Man",
                "aliases": ["The Fall of Man", "Humanity Falls"],
                "anchor_ref": "Genesis 3:2",
                "target_ranges": [
                    {
                        "start_ref": "Genesis 3:1",
                        "end_ref": "Genesis 3:2",
                    }
                ],
            },
            {
                "key": "creation story",
                "label": "Creation Story",
                "aliases": [],
                "anchor_ref": "Genesis 1:1",
                "target_ranges": [
                    {
                        "start_ref": "Genesis 1:1",
                        "end_ref": "Genesis 1:2",
                    }
                ],
            },
        ]
        write_json(curated, records)
        output = self.root / "curated-overlay.jsonl"
        with self.assertRaisesRegex(ValueError, "provenance-free direct mappings"):
            overlay.write_overlay(
                canonical_path=self.canonical,
                output_path=output,
                alternates=[],
                include_events=False,
                curated_concepts_path=curated,
            )
        self.assertFalse(output.exists())

    def test_ref_only_canonical_derives_bcv_for_ranges_and_alignment(self) -> None:
        canonical = self.root / "ref-only.jsonl"
        write_jsonl(
            canonical,
            [
                {"ref": "Genesis 1:1", "text": "Canonical one"},
                {"ref": "Genesis 1:2", "text": "Canonical two"},
            ],
        )
        verses = overlay.load_canonical(canonical)
        self.assertEqual(
            [("genesis", "1", "1"), ("genesis", "1", "2")],
            [verse.bcv_key for verse in verses],
        )

        alternate = self.root / "structured-alternate.jsonl"
        write_jsonl(
            alternate,
            [
                {
                    "ref": "Gen.1.1",
                    "book": "Genesis",
                    "chapter": 1,
                    "verseNum": 1,
                    "text": "Alternate one",
                },
                {
                    "ref": "Gen.1.2",
                    "book": "Genesis",
                    "chapter": 1,
                    "verseNum": 2,
                    "text": "Alternate two",
                },
            ],
        )
        alignment = overlay.align_alternate(
            overlay.AlternateSource("KJV", alternate), verses
        )
        self.assertEqual(2, len(alignment.records))
        self.assertEqual(
            ["Genesis 1:1", "Genesis 1:2"],
            [record["target_ref"] for record in alignment.records],
        )


if __name__ == "__main__":
    unittest.main()
