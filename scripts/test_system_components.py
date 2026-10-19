import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from system_components import ROOT, load_model, report


class ComponentsTest(unittest.TestCase):
    def test_plans_are_dependency_ordered_and_never_deploy_system_components(self):
        model = load_model(ROOT / 'release/system-components.json')
        install = report(model, 'install', 'all')['components']
        remove = report(model, 'remove', 'all')['components']
        self.assertEqual([c['id'] for c in remove], list(reversed([c['id'] for c in install])))
        self.assertEqual([c['id'] for c in install if c['handler']], ['app'])
        self.assertTrue(all(c['execution'] == 'plan-only' for c in install))

    def test_missing_and_modified_files_do_not_claim_verified_installation(self):
        model = load_model(ROOT / 'release/system-components.json')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            exe = root / 'capyio-desktop.exe'
            exe.write_bytes(b'fixture')
            (root / 'manifest.json').write_text(json.dumps({'schemaVersion': 3, 'artifacts': [
                {'file': exe.name, 'sha256': hashlib.sha256(b'fixture').hexdigest()}]}))
            result = report(model, 'verify', 'core', root)
            self.assertEqual(result['components'][0]['artifactState'], 'verified')
            self.assertIsNone(result['components'][0]['installedVersion'])
            self.assertEqual(result['components'][0]['systemState'], 'not-probed')
            self.assertEqual(result['components'][2]['artifactState'], 'not-verified')
            exe.write_bytes(b'changed')
            self.assertFalse(report(model, 'verify', 'core', root)['bundleIntegrity'])


if __name__ == '__main__':
    unittest.main()
