import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from check_architecture import violations, ratchet, source_files


class ArchitectureTests(unittest.TestCase):
    def test_new_source_cannot_hide_growth_in_comments_or_blank_lines(self):
        for contents in ["// comment\n" * 801, "\n" * 801, "fn code() {}\n" * 801]:
            self.assertEqual(len(violations({"crate/src/new.rs": contents}, {})), 1)

    def test_bounds_apply_to_each_file_not_the_directory_total(self):
        files = {"crate/src/one.rs": "line\n" * 800, "crate/src/two.rs": "line\n" * 800}
        self.assertEqual(violations(files, {}), [])
        files["crate/src/one.rs"] += "line\n"
        self.assertEqual(len(violations(files, {})), 1)

    def test_test_modules_are_bounded_too(self):
        self.assertEqual(violations({"crate/tests/business.rs": "line\n" * 1000}, {}), [])
        self.assertEqual(len(violations({"crate/tests/business.rs": "line\n" * 1001}, {})), 1)

    def test_existing_debt_cannot_grow_or_expand_to_new_files(self):
        prior = {"crate/src/old.rs": 1200}
        self.assertEqual(ratchet(prior, {"crate/src/old.rs": 1100}), [])
        self.assertEqual(ratchet(prior, {}), [])
        self.assertEqual(len(ratchet(prior, {"crate/src/old.rs": 1201})), 1)
        self.assertEqual(len(ratchet(prior, {"crate/src/new.rs": 1200})), 1)
        self.assertEqual(len(violations({}, prior)), 1)

    def test_only_diesel_schema_is_exempt_even_for_untracked_sources(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            crate = root / "db-sqlite"
            (crate / "src").mkdir(parents=True)
            (crate / "Cargo.toml").touch()
            (crate / "src/schema.rs").write_text("generated\n" * 2000)
            (crate / "src/generated.rs").write_text("handwritten\n" * 801)
            self.assertEqual(len(violations(source_files(root), {})), 1)


if __name__ == "__main__":
    unittest.main()
