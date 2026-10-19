"""Verify an Extras inventory and optional matching Core; no installation."""
import argparse
import hashlib
import json
from pathlib import Path


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def verify(root, core=None):
    root = root.resolve()
    manifest = json.loads((root / 'manifest.json').read_text(encoding='utf-8-sig'))
    if (manifest['schemaVersion'] != 1 or manifest['kind'] != 'capyio-local-experimental-extras'
            or manifest['publicationApproved'] or manifest['automaticOverlaySupported']):
        raise ValueError('Unsupported Extras policy')
    seen = {'manifest.json'}
    for entry in manifest['artifacts']:
        name = entry['file']
        path = root / name
        if name in seen or not path.resolve().is_relative_to(root) or path.is_symlink():
            raise ValueError('Invalid Extras path')
        if path.stat().st_size != entry['bytes'] or digest(path) != entry['sha256'].lower():
            raise ValueError(f'Integrity mismatch: {name}')
        seen.add(name)
    actual = {p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file()}
    if actual != seen:
        raise ValueError('Unlisted Extras files')
    if core and digest(core / 'manifest.json') != manifest['requiresCoreManifestSha256']:
        raise ValueError('Extras does not match this Core inventory')
    return len(seen) - 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--core', type=Path)
    args = parser.parse_args()
    print(f'Extras integrity PASS: {verify(args.root, args.core)} files; not installation or license approval.')
