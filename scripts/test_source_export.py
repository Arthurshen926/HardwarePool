import json
import tempfile
import unittest
from pathlib import Path
from export_source_candidate import ROOT, digest, scan, selected, verify, review_group, apply_decisions


class SourceExportTest(unittest.TestCase):
    def test_allowlist_excludes_local_state_and_credentials(self):
        policy = json.loads((ROOT / 'release/source-export-policy.json').read_text())
        for name in ['.git/config', '.codex/history.json', 'target/log.json', 'platform/android/local.properties',
                     'platform/android/app/build/a.java', 'apps/desktop/.env', 'fixtures/local.pem', 'CapyIO_Codex_Master_Prompt.md']:
            self.assertFalse(selected(name, policy), name)
        for name in ['Cargo.lock', 'LICENSES/MIT.txt', 'crates/capyio-core/src/lib.rs', '.vscode/extensions.json']:
            self.assertTrue(selected(name, policy), name)

    def test_findings_never_include_matched_secret_or_personal_value(self):
        secret = '-----BEGIN ' + 'PRIVATE KEY-----'
        data = (secret + '\n' + 'C:' + '/Users/' + 'fixture-person/file').encode()
        findings = scan('fixture.txt', data)
        self.assertEqual({f['category'] for f in findings}, {'private-key', 'personal-windows-path'})
        self.assertNotIn('fixture-person', json.dumps(findings))
        self.assertNotIn(secret, json.dumps(findings))

    def test_classification_keeps_credential_priority(self):
        for name in ('scripts/example.test.mjs', 'scripts/example.test.ts', 'scripts/example.test.js'):
            self.assertEqual(review_group(name, 'private-network-address'), 'test-fixture-review')
            self.assertEqual(review_group(name, 'private-key'), 'credential-review')
        self.assertEqual(review_group('platform/android/README.md', 'private-network-address'), 'documentation-review')
        self.assertEqual(review_group('drivers/example/AGENTS.md', 'machine-name'), 'documentation-review')
        self.assertEqual(review_group('crates/example/src/lib.rs', 'private-network-address'), 'implementation-review')

    def test_review_decision_expires_when_source_changes(self):
        data = b'example 192.168.1.20'
        finding = scan('example.txt', data)[0]
        decisions = [{**finding, 'fileSha256': digest(data),
                      'disposition': 'retain-explained', 'reason': 'Example only.'}]
        self.assertEqual(apply_decisions(scan('example.txt', data), data, decisions)[0]['reviewStatus'], 'retain-explained')
        changed = data + b' changed'
        self.assertEqual(apply_decisions(scan('example.txt', changed), changed, decisions)[0]['reviewStatus'], 'pending')
        self.assertEqual(apply_decisions(scan('other.txt', data), data, decisions)[0]['reviewStatus'], 'pending')

    def test_integrity_rejects_unlisted_and_modified_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            tree = base / 'candidate'
            tree.mkdir()
            (tree / 'source.rs').write_bytes(b'source')
            (base / 'manifest.json').write_text(json.dumps({'kind': 'source-review-candidate',
                'publicationApproved': False, 'historyIncluded': False,
                'artifacts': [{'file': 'source.rs', 'bytes': 6, 'sha256': digest(b'source')}]}))
            self.assertEqual(verify(base), 1)
            (tree / 'unlisted').write_bytes(b'x')
            with self.assertRaises(ValueError):
                verify(base)
            (tree / 'source.rs').write_bytes(b'changed')
            with self.assertRaises(ValueError):
                verify(base)


if __name__ == '__main__':
    unittest.main()
