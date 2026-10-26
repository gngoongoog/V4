"""Deterministic application source identity, independently checked against Rust."""
from pathlib import Path
import hashlib
import json
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def fingerprint(root=ROOT):
    files = set()
    def visit(relative):
        if '__pycache__' in Path(relative).parts or relative.endswith(('.pyc', '.pyo')):
            return
        p = root / relative
        if p.is_symlink():
            raise ValueError('Release input must not be a symlink: ' + relative)
        if p.is_dir():
            for child in p.iterdir():
                visit(child.relative_to(root).as_posix())
        elif p.is_file():
            files.add(relative)
        else:
            raise ValueError('Missing release input: ' + relative)
    for item in json.loads((root / 'release-inputs.json').read_text('utf-8')):
        visit(item)
    h = hashlib.sha256(b'braintrade-source-v1\0')
    for name in sorted(files):
        data = (root / name).read_bytes().decode('utf-8').replace('\r\n', '\n').encode('utf-8')
        path = name.encode('utf-8')
        h.update(len(path).to_bytes(8, 'big')); h.update(path)
        h.update(len(data).to_bytes(8, 'big')); h.update(data)
    return h.hexdigest()


def identity(root=ROOT):
    release = json.loads((root / 'release.json').read_text('utf-8'))
    return dict(version=release['version'], build_id=fingerprint(root),
                core_protocol=release['core_protocol'], release=release['name'],
                channel=release['channel'], marker=release['marker'])


def validate_versions(root=ROOT):
    expected = json.loads((root / 'release.json').read_text('utf-8'))['version']
    workspace = tomllib.loads((root / 'Cargo.toml').read_text('utf-8'))
    assert workspace['workspace']['package']['version'] == expected
    for relative in ('apps/terminal/package.json', 'apps/terminal/package-lock.json',
                     'apps/tauri/tauri.conf.json'):
        assert json.loads((root / relative).read_text('utf-8'))['version'] == expected, relative
    lock = tomllib.loads((root / 'Cargo.lock').read_text('utf-8'))
    assert all(p['version'] == expected for p in lock['package'] if p['name'].startswith('braintrade-'))
    npm_lock = json.loads((root / 'apps/terminal/package-lock.json').read_text('utf-8'))
    assert npm_lock['packages']['']['version'] == expected
    tauri = tomllib.loads((root / 'apps/tauri/Cargo.toml').read_text('utf-8'))
    assert tauri['package']['version'] == {'workspace': True}


if __name__ == '__main__':
    validate_versions()
    print(json.dumps(identity(), indent=2))
