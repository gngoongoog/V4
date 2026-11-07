"""Generate an offline, version-scoped operator checklist; no Core/MT5 access."""
from __future__ import annotations
import argparse
import hashlib
import html
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def build(output: Path, identity: dict, commit: str):
    document = (ROOT / 'docs/V26_WINDOWS_MT5_CHECKLIST_AR.md').read_text(encoding='utf-8')
    sections = []
    current = None
    identifiers = set()
    for line in document.splitlines():
        if line.startswith('## '):
            current = {'title': line[3:], 'items': []}
            sections.append(current)
        match = re.match(r'^\| ([A-Z]\d{2}) \| (.*?) \| (.*?) \|', line)
        listed = re.match(r'^- \[ \] ([A-Z]\d{2}): (.+)', line)
        if match:
            identifier, action, expected = match.groups()
        elif listed:
            identifier, action = listed.groups()
            expected = ''
        else:
            continue
        if current is None or identifier in identifiers:
            raise ValueError('Checklist section or item identity invalid')
        identifiers.add(identifier)
        current['items'].append(dict(id=identifier, action=action, expected=expected))
    sections = [section for section in sections if section['items']]
    if not sections:
        raise ValueError('No checklist items found')
    payload = dict(schema='braintrade-user-checklist-v1', source_commit=commit,
                   build_id=identity['build_id'], product_version=identity['version'],
                   checklist_sha256=hashlib.sha256(document.encode()).hexdigest(),
                   sections=sections)
    template = (ROOT / 'tools/operator-checklist.html').read_text(encoding='utf-8')
    encoded = json.dumps(payload, ensure_ascii=False).replace('<', '\\u003c').replace('>', '\\u003e').replace('&', '\\u0026')
    result = template.replace('__CHECKLIST_DATA__', encoded)
    result = result.replace('__BUILD_SHORT__', html.escape(commit[:7]))
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(result, encoding='utf-8')
    return {'file': str(output), 'items': len(identifiers), 'build_id': identity['build_id']}


if __name__ == '__main__':
    from release_identity import identity
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    print(json.dumps(build(args.output, identity(), commit)))
