import unittest

import cli_audit


class AuditTests(unittest.TestCase):
    def test_cli_security_findings_block_while_other_components_remain_reported(self):
        report = {
            "database": {},
            "vulnerabilities": {
                "list": [
                    {
                        "package": {"name": "cli-dependency", "version": "1.0.0"},
                        "advisory": {"id": "RUSTSEC-test-cli"},
                    },
                    {
                        "package": {"name": "backend-only", "version": "1.0.0"},
                        "advisory": {"id": "RUSTSEC-test-backend"},
                    },
                ]
            },
            "warnings": {
                "unmaintained": [
                    {
                        "package": {"name": "cli-maintenance", "version": "2.0.0"},
                        "advisory": {"id": "RUSTSEC-test-maintenance"},
                    }
                ],
                "yanked": [{"package": {"name": "cli-yanked", "version": "3.0.0"}}],
            },
        }
        blocking, review, outside = cli_audit.classify(
            report,
            {
                ("cli-dependency", "1.0.0"),
                ("cli-maintenance", "2.0.0"),
                ("cli-yanked", "3.0.0"),
            },
        )
        self.assertEqual(
            {p["name"] for p in blocking}, {"cli-dependency", "cli-yanked"}
        )
        self.assertEqual([p["name"] for p in review], ["cli-maintenance"])
        self.assertEqual([p["name"] for p in outside], ["backend-only"])

    def test_missing_report_data_cannot_produce_clean_result(self):
        with self.assertRaises(ValueError):
            cli_audit.classify({}, set())


if __name__ == "__main__":
    unittest.main()
