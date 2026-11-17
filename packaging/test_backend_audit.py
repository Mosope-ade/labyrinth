"""Prove metadata-only optional drivers cannot enter the compiled audit scope."""

import unittest

import backend_audit
import cli_audit


class ScopeTests(unittest.TestCase):
    def setUp(self):
        self.packages = [
            {"id": n, "name": n, "version": "0.1.0", "source": None}
            for n in backend_audit.COMPONENTS
        ] + [
            {"id": n, "name": n, "version": "1.0.0", "source": "registry"}
            for n in ["compiled-crypto", "inactive-sqlite"]
        ]
        self.units = backend_audit.COMPONENTS | {"compiled-crypto"}

    def test_optional_metadata_does_not_imply_compilation(self):
        dependencies = backend_audit.scope({"packages": self.packages}, self.units)
        self.assertEqual(dependencies, {("compiled-crypto", "1.0.0")})
        report = {
            "database": {},
            "warnings": {},
            "vulnerabilities": {
                "list": [
                    {"package": {"name": n, "version": "1.0.0"}, "advisory": {"id": n}}
                    for n in ["compiled-crypto", "inactive-sqlite"]
                ]
            },
        }
        blocking, review, outside = cli_audit.classify(report, dependencies)
        self.assertEqual([v["name"] for v in blocking], ["compiled-crypto"])
        self.assertEqual([v["name"] for v in outside], ["inactive-sqlite"])
        self.assertFalse(review)

    def test_incomplete_and_unknown_compiler_evidence_fails_closed(self):
        for units in [
            set(),
            self.units - {"labyrinth-engine"},
            self.units | {"unknown"},
        ]:
            with self.assertRaises(ValueError):
                backend_audit.scope({"packages": self.packages}, units)
