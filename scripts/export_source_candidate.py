"""Export a local review candidate without Git history; never publish or redact source."""
import argparse
from collections import Counter
import hashlib
import json
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PATTERNS = {
    'private-key': re.compile(r'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----'),
    'aws-access-key': re.compile(r'AKIA[0-9A-Z]{16}'),
    'personal-windows-path': re.compile(r'[A-Za-z]:[\\/]+Users[\\/]+[^\\/\s"\']+', re.I),
    'personal-unix-path': re.compile(r'(?<!:)/(?:home|Users)/[A-Za-z0-9_.-]+/'),
    'user-sid': re.compile(r'\bS-1-5-21-\d+-\d+-\d+-\d+\b'),
    'machine-name': re.compile(r'\b(?:DESKTOP|LAPTOP)-[A-Z0-9]{5,}\b'),
    'private-network-address': re.compile(r'\b(?:10(?:\.\d{1,3}){3}|192\.168(?:\.\d{1,3}){2}|172\.(?:1[6-9]|2\d|3[01])(?:\.\d{1,3}){2}|100\.(?:6[4-9]|[7-9]\d|1[01]\d|12[0-7])(?:\.\d{1,3}){2})\b'),
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def selected(name, policy):
    path = Path(name)
    if any(part in policy['excludedParts'] for part in path.parts):
        return False
    if path.suffix.lower() in policy['excludedSuffixes'] or path.name == 'local.properties' or path.name.startswith('.env'):
        return False
    return name in policy['rootFiles'] or (len(path.parts) > 1 and path.parts[0] in policy['roots'])


def review_group(name, category):
    if category in ('private-key', 'aws-access-key'):
        return 'credential-review'
    if category == 'binary-review':
        return 'binary-resource-review'
    if name.startswith('docs/') or Path(name).name in ('README.md', 'AGENTS.md'):
        return 'documentation-review'
    if '/tests/' in name or '/contractTest/' in name or Path(name).name.startswith('test_') or name.endswith(('.test.mjs', '.test.ts', '.test.js')):
        return 'test-fixture-review'
    return 'implementation-review'


def scan(name, data):
    try:
        text = data.decode('utf-8-sig')
    except UnicodeDecodeError:
        return [{'file': name, 'line': None, 'category': 'binary-review', 'reviewGroup': review_group(name, 'binary-review')}]
    findings = []
    for line, value in enumerate(text.splitlines(), 1):
        for category, pattern in PATTERNS.items():
            if pattern.search(value):
                findings.append({'file': name, 'line': line, 'category': category, 'reviewGroup': review_group(name, category)})
    return findings


def apply_decisions(findings, data, decisions):
    file_hash = digest(data)
    for finding in findings:
        finding['reviewStatus'] = 'pending'
        for decision in decisions:
            if (decision['fileSha256'] == file_hash and
                    all(decision[key] == finding[key] for key in ('file', 'line', 'category'))):
                finding['reviewStatus'] = decision['disposition']
                finding['reviewReason'] = decision['reason']
                break
    return findings


def export(root=ROOT):
    policy = json.loads((root / 'release/source-export-policy.json').read_text(encoding='utf-8'))
    if policy['schemaVersion'] != 1 or policy['publicationApproved']:
        raise ValueError('Unsupported export policy')
    decisions_path = root / 'release/source-review-decisions.json'
    decisions = json.loads(decisions_path.read_text(encoding='utf-8'))['decisions'] if decisions_path.exists() else []
    names = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root).decode().split('\0')
    base = root / 'target/source-export' / datetime.now(timezone.utc).strftime('%Y%m%d-%H%M%S-%f')
    tree = base / 'candidate'
    tree.mkdir(parents=True, exist_ok=False)
    inventory, findings, excluded = [], [], []
    for name in sorted(set(filter(None, names))):
        if not selected(name, policy):
            excluded.append(name)
            continue
        source = root / name
        if not source.is_file():
            continue  # Tracked deletion is not restored from history.
        if source.is_symlink() or not source.resolve().is_relative_to(root.resolve()):
            raise ValueError(f'Export input is not an ordinary repository file: {name}')
        data = source.read_bytes()
        destination = tree / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        inventory.append({'file': name, 'bytes': len(data), 'sha256': digest(data)})
        findings.extend(apply_decisions(scan(name, data), data, decisions))
    manifest = {'schemaVersion': 1, 'kind': 'source-review-candidate', 'publicationApproved': False,
                'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
                'workingTreeIncluded': True, 'historyIncluded': False,
                'policySha256': digest((root / 'release/source-export-policy.json').read_bytes()),
                'artifacts': inventory}
    report = {'schemaVersion': 1, 'publicationApproved': False, 'files': len(inventory),
              'excludedFiles': excluded, 'findings': findings,
              'reviewCountsByGroup': dict(Counter(item['reviewGroup'] for item in findings)),
              'reviewCountsByStatus': dict(Counter(item['reviewStatus'] for item in findings)),
              'limitations': ['Heuristic scan; absence of findings is not privacy or license clearance.',
                              'Binary files require review; source bytes are not silently rewritten.',
                              'Excluded outputs are not corresponding-source closure for optional native engines.']}
    for name, value in [('manifest.json', manifest), ('review.json', report)]:
        (base / name).write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps({'directory': str(base), 'files': len(inventory), 'reviewFindings': len(findings),
                      'publicationApproved': False}))


def verify(base):
    tree = base / 'candidate'
    manifest = json.loads((base / 'manifest.json').read_text(encoding='utf-8'))
    if manifest['kind'] != 'source-review-candidate' or manifest['publicationApproved'] or manifest['historyIncluded']:
        raise ValueError('Unsupported candidate policy')
    expected = set()
    for entry in manifest['artifacts']:
        path = tree / entry['file']
        if path.is_symlink() or not path.resolve().is_relative_to(tree.resolve()):
            raise ValueError('Invalid candidate path')
        data = path.read_bytes()
        if len(data) != entry['bytes'] or digest(data) != entry['sha256']:
            raise ValueError('Candidate file differs from inventory')
        expected.add(entry['file'])
    actual = {p.relative_to(tree).as_posix() for p in tree.rglob('*') if p.is_file()}
    if expected != actual or (tree / '.git').exists():
        raise ValueError('Candidate contains unlisted files or Git history')
    return len(expected)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--verify', type=Path)
    args = parser.parse_args()
    if args.verify:
        print(f'Source candidate integrity PASS: {verify(args.verify)} files; publication is not approved.')
    else:
        export()
