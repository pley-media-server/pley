"""Cuts Noto Sans SC and TC to the characters Pley for Roku writes.

Roboto and the Roku's own font have no Chinese characters, so the Roku app
draws Chinese in Noto Sans SC and TC (OFL-1.1, google/fonts ofl/notosanssc
and ofl/notosanstc), cut to what it needs: each script's font gets its own
language's words (apps/roku/locales/zh-CN, zh-TW), the characters the code
writes itself, both languages' names, and printable ASCII and Latin-1, as
static weights 400, 500 and 700 (Roboto's regular, medium and bold).

  pip install fonttools
  python scripts/subset-roku-cjk.py <dir with NotoSansSC[wght].ttf and NotoSansTC[wght].ttf>

Run it again when the Chinese words change.
"""
import glob
import json
import os
import sys

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

root = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
app = os.path.join(root, 'apps', 'roku')
out = os.path.join(app, 'fonts')
fonts = sys.argv[1]
os.makedirs(out, exist_ok=True)


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for v in value.values():
            yield from strings(v)
    elif isinstance(value, list):
        for v in value:
            yield from strings(v)


def chars_of(language):
    text = set()
    for path in glob.glob(os.path.join(app, 'locales', language, '*.json')):
        with open(path, encoding='utf-8') as f:
            for s in strings(json.load(f)):
                text.update(s)
    return text


code = set()
for path in glob.glob(os.path.join(app, 'components', '**', '*.brs'), recursive=True):
    with open(path, encoding='utf-8') as f:
        code.update(c for c in f.read() if ord(c) > 0x2E7F)
base = {chr(c) for c in range(0x20, 0x7F)} | {chr(c) for c in range(0xA0, 0x100)}
base |= set('简体中文繁體中文') | set('，。、：；！？「」『』（）《》〈〉…—·％　')

for script, language in [('SC', 'zh-CN'), ('TC', 'zh-TW')]:
    chars = base | code | chars_of(language)
    source = os.path.join(fonts, f'NotoSans{script}[wght].ttf')
    for weight in (400, 500, 700):
        font = TTFont(source)
        static = instancer.instantiateVariableFont(font, {'wght': weight})
        options = subset.Options()
        options.layout_features = ['*']
        options.name_IDs = ['*']
        options.notdef_outline = True
        subsetter = subset.Subsetter(options)
        subsetter.populate(text=''.join(sorted(chars)))
        subsetter.subset(static)
        target = os.path.join(out, f'noto-{script.lower()}-{weight}.ttf')
        # The source's date, not today's: the same fonts each run.
        static.recalcTimestamp = False
        static.save(target)
        print(target, len(chars), 'characters', os.path.getsize(target) // 1024, 'KB')
