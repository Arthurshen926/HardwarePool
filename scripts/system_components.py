"""Read-only component inventory and closed operation planning; never deploys."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OPERATIONS = ('check', 'verify', 'install', 'upgrade', 'rollback', 'remove')
APP_HANDLERS = {
    'install': 'install_local_debug_bundle.ps1',
    'upgrade': 'update_local_debug_bundle.ps1 -Mode Upgrade',
    'rollback': 'update_local_debug_bundle.ps1 -Mode Rollback',
    'remove': 'remove_local_debug_bundle.ps1',
}


def load_model(path):
    model = json.loads(path.read_text(encoding='utf-8-sig'))
    if model['schemaVersion'] != 1 or model['publicationApproved'] or not model['preserveUserData']:
        raise ValueError('Unsupported release policy')
    entries = model['components']
    ids = [c['id'] for c in entries]
    if len(set(ids)) != len(ids):
        raise ValueError('Duplicate component ID')
    seen = set()
    for c in entries:
        if c['group'] not in ('core', 'extras') or not set(c['requires']) <= seen:
            raise ValueError('Components must be dependency ordered')
        if c['operations'] != {op: ('application-files' if c['id'] == 'app' else 'prerequisite-only') for op in APP_HANDLERS}:
            raise ValueError('Unreviewed operation handler')
        seen.add(c['id'])
    return model


def inspect_bundle(root):
    manifest = json.loads((root / 'manifest.json').read_text(encoding='utf-8-sig'))
    if manifest.get('schemaVersion') != 3:
        raise ValueError('Expected schema-v3 application bundle')
    records = {}
    for entry in manifest['artifacts']:
        name = entry['file']
        path = (root / name).resolve()
        if not path.is_relative_to(root.resolve()) or name in records:
            raise ValueError('Unsafe or duplicate artifact path')
        expected = entry['sha256'].lower()
        actual = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None
        records[name] = {'expectedSha256': expected, 'actualSha256': actual,
                         'state': 'verified' if actual == expected else 'mismatch' if actual else 'missing'}
    return manifest, records


def report(model, operation, profile, root=None):
    bundle, artifacts = inspect_bundle(root) if root else ({}, {})
    selected = [c for c in model['components'] if profile == 'all' or c['group'] == profile]
    if operation == 'remove':
        selected.reverse()
    results = []
    for c in selected:
        evidence = {name: artifacts.get(name, {'state': 'not-in-bundle'}) for name in c['files']}
        files_ok = bool(evidence) and all(v['state'] == 'verified' for v in evidence.values())
        results.append({
            'id': c['id'], 'required': c['required'], 'kind': c['kind'],
            'releaseVersion': model['releaseVersion'], 'installedVersion': None,
            'artifactState': 'verified' if files_ok else 'not-verified',
            'systemState': 'not-probed', 'artifacts': evidence,
            'operation': operation,
            'handler': APP_HANDLERS.get(operation) if c['id'] == 'app' else None,
            'execution': 'read-only' if operation in ('check', 'verify') else 'plan-only',
            'requires': c['requires'], 'restart': c['restart'], 'limitation': c['limitation'],
        })
    return {'schemaVersion': 1, 'profile': profile, 'operation': operation,
            'sourceRevision': bundle.get('sourceRevision'),
            'buildProvenance': bundle.get('buildProvenance'),
            'packageManifestSha256': hashlib.sha256((root / 'manifest.json').read_bytes()).hexdigest() if root else None,
            'preserveUserData': True, 'publicationApproved': False,
            'bundleIntegrity': all(v['state'] == 'verified' for v in artifacts.values()) if root else None,
            'components': results}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=OPERATIONS)
    parser.add_argument('--profile', choices=('core', 'extras', 'all'), default='core')
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--manifest', type=Path, default=ROOT / 'release/system-components.json')
    args = parser.parse_args()
    if args.operation == 'verify' and not args.bundle:
        parser.error('verify requires --bundle; no install is inferred from files')
    result = report(load_model(args.manifest), args.operation, args.profile, args.bundle)
    print(json.dumps(result, indent=2))
    if args.operation == 'verify' and (not result['bundleIntegrity'] or any(c['artifacts'] and c['artifactState'] != 'verified' for c in result['components'])):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
