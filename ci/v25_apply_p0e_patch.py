"""Apply narrow V25 native EA repair hunks to restored V24 sources.

Git applies full context too strictly to our V18+V23+V24 reconstructed source.
This tool trims ONLY unchanged edge context until every hunk has an exact,
unique original anchor. Ambiguity or a missing source anchor fails the build.
No binary/state file can be edited and no broker API is imported.
"""
from pathlib import Path
import re

PATCH=Path('ci/v25_p0e_native_reconnect.patch')
ALLOWED={'gateway/sniper_native.py','gateway/mt5_gateway.py'}
HEADER=re.compile(r'^@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@')


def parse_hunks(patch):
    result={}
    path=None
    rows=[]
    def flush():
        if path is not None and rows:
            result.setdefault(path, []).append(tuple(rows))
        rows.clear()
    for line in patch.splitlines(keepends=True):
        if line.startswith('--- a/'):
            flush()
            path=line[len('--- a/'):].strip()
            if path not in ALLOWED:
                raise SystemExit(f'P0-E unexpected patch target: {path}')
        elif line.startswith('+++ b/'):
            expected=line[len('+++ b/'):].strip()
            if expected != path:
                raise SystemExit('P0-E mismatched diff file headers')
        elif HEADER.match(line):
            flush()
        elif path is not None and line[:1] in (' ', '+', '-'):
            rows.append(line)
        elif line.startswith('\\ No newline'):
            raise SystemExit('P0-E no-newline patch unsupported')
    flush()
    if set(result) != ALLOWED:
        raise SystemExit('P0-E expected both audited files, no others')
    return result


def apply_hunk(content, rows, target, number):
    def render(group):
        old=''.join(x[1:] for x in group if x.startswith((' ','-')))
        new=''.join(x[1:] for x in group if x.startswith((' ','+')))
        return old,new
    group=list(rows)
    if not any(row[0] in '+-' for row in group):
        raise SystemExit(f'P0-E no changes in hunk {target}:{number}')
    while True:
        old,new=render(group)
        if old and content.count(old)==1:
            return content.replace(old,new,1)
        leading=bool(group and group[0].startswith(' '))
        trailing=bool(group and group[-1].startswith(' '))
        if not leading and not trailing:
            raise SystemExit(f'P0-E ambiguous/missing exact anchor: {target} hunk {number}')
        if leading and trailing:
            left=group[0][1:]
            right=group[-1][1:]
            if content.count(left) >= content.count(right):
                group.pop(0)
            else:
                group.pop()
        elif leading:
            group.pop(0)
        else:
            group.pop()


def main():
    hunks=parse_hunks(PATCH.read_text(encoding='utf-8'))
    staged={}
    for path, items in hunks.items():
        target=Path(path)
        data=target.read_bytes()
        if b'\r\n' in data:
            data=data.replace(b'\r\n',b'\n')
        text=data.decode('utf-8')
        for no, rows in enumerate(items,1):
            text=apply_hunk(text,rows,path,no)
        staged[target]=text
    for path,content in staged.items():
        if path.name=='sniper_native.py':
            if 'self.blocked = True' not in content or 'self.mailbox.reset()' not in content:
                raise SystemExit('P0-E Native pipe postcondition failed')
        else:
            if 'native.recent(login=' not in content or 'account_fields=' not in content:
                raise SystemExit('P0-E MT5 session postcondition failed')
    for path,content in staged.items():
        path.write_text(content,encoding='utf-8')
        print(f'P0-E verified minimal {path}: {len(hunks[path.as_posix()])} hunks')


if __name__=='__main__':
    main()
