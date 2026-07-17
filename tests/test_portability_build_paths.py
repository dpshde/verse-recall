from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]


class PortabilityBuildPathTests(unittest.TestCase):
    def test_android_imported_library_is_not_generated_under_jnilibs(self) -> None:
        build_script = (
            ROOT / "bindings/kotlin/scripts/build-rust-android.sh"
        ).read_text(encoding="utf-8")
        gradle = (
            ROOT / "bindings/kotlin/verse-recall/build.gradle.kts"
        ).read_text(encoding="utf-8")
        cmake = (
            ROOT / "bindings/kotlin/verse-recall/src/main/cpp/CMakeLists.txt"
        ).read_text(encoding="utf-8")

        self.assertIn("verse-recall/build/generated/rustJniLibs", build_script)
        self.assertNotIn("verse-recall/src/main/jniLibs", build_script)
        self.assertIn('layout.buildDirectory.dir("generated/rustJniLibs")', gradle)
        self.assertIn("-DRECALL_RUST_OUTPUT=", gradle)
        self.assertIn("rustOutput.get().asFile,", gradle)
        self.assertIn("${RECALL_RUST_OUTPUT}/${ANDROID_ABI}", cmake)
        self.assertNotIn("../jniLibs", cmake)

    def test_swift_local_test_uses_one_explicit_cargo_target_directory(self) -> None:
        script = (ROOT / "bindings/swift/scripts/test-local.sh").read_text(
            encoding="utf-8"
        )

        self.assertIn('CARGO_TARGET_DIR="$RUST_TARGET_DIR"', script)
        self.assertIn('--target "$HOST_TRIPLE"', script)
        self.assertIn('RUST_LIBRARY_DIR="$RUST_TARGET_DIR/$HOST_TRIPLE/debug"', script)
        self.assertNotIn('$REPO_DIR/target/debug', script)


if __name__ == "__main__":
    unittest.main()
