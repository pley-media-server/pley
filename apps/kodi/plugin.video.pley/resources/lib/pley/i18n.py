"""The languages Pley is in (English, French and Spanish), and the add-on's
words in each: its strings.po files, read here rather than through Kodi.

Kodi's getLocalizedString gives an add-on's words in Kodi's own language
only, and Pley shows the account's when it has one (docs/API.md,
Language), so the add-on reads its strings.po files itself. They keep
Kodi's layout (resources/language/resource.language.<code>/strings.po,
each word by its msgctxt "#<id>"), so Kodi's settings screen, which Kodi
draws itself, reads them as any add-on's.

A word that counts has three ids in a row, one for each plural form CLDR
gives French and Spanish: one, many (a whole number of millions), other.
English, which has only one and other, writes many as it writes other.

No Kodi here: kodi.py says which language is shown."""

from __future__ import annotations

import os
import re

ENGLISH = "en"
LANGUAGES = (ENGLISH, "fr", "es")
# Kodi's folder for each language's strings.po.
FOLDERS = {"en": "resource.language.en_gb", "fr": "resource.language.fr_fr", "es": "resource.language.es_es"}
# Where the folders are: resources/language, beside resources/lib.
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "language")

# The plural forms, in the order of a counted word's ids.
FORMS = ("one", "many", "other")

# How each language writes numbers: what groups the thousands, and the
# decimal mark. A no-break space for French, as CLDR has it, where Kodi's
# fonts have the narrow one only sometimes.
GROUPS = {"en": ",", "fr": " ", "es": "."}
POINTS = {"en": ".", "fr": ",", "es": ","}


def supported(tag) -> str | None:
    """The language Pley has for a tag ("fr-CA", "FR_ca", "fr" → "fr"), if any."""
    if not isinstance(tag, str):
        return None
    base = re.split(r"[-_.]", tag.strip(), maxsplit=1)[0].lower()
    return base if base in LANGUAGES else None


def device_language(tags) -> str | None:
    """The first of the device's languages, its own first, that Pley has."""
    for tag in tags or ():
        found = supported(tag)
        if found:
            return found
    return None


def automatic(server, device) -> str:
    """What Automatic is here: the device's language where Pley has it,
    else the server's, else English."""
    return device_language(device) or supported(server) or ENGLISH


def pick(account, server, device, choice=None) -> str:
    """The language shown (packages/tv/test/fixtures/language.json): the
    account's own, else the one chosen on this device, else the device's
    where Pley has it, else the server's, else English. A language Pley
    isn't in counts as none."""
    return supported(account) or supported(choice) or automatic(server, device)


def plural_form(language: str, n) -> str:
    """The plural form of `n`, as CLDR has it: English "one" is exactly 1;
    French "one" is 0 to 1.99…; Spanish "one" is 1. French and Spanish say
    "many" of a whole number of millions."""
    value = abs(n)
    whole = int(value)
    if language == ENGLISH:
        return "one" if value == 1 else "other"
    if (whole in (0, 1)) if language == "fr" else value == 1:
        return "one"
    return "many" if whole == value and whole and whole % 1_000_000 == 0 else "other"


def number(n, language: str) -> str:
    """A whole number as `language` writes it: "12,345", "12 345", "12.345".
    Spanish groups from five digits ("1234")."""
    text = "{:,}".format(int(n))
    if language == "es" and abs(int(n)) < 10_000:
        return text.replace(",", "")
    return text.replace(",", GROUPS.get(language, ","))


def decimal(value, digits: int, language: str) -> str:
    """`value` with `digits` decimals, as `language` writes it: "7.4", "7,4"."""
    return ("%.*f" % (digits, value)).replace(".", POINTS.get(language, "."))


# ------------------------------------------------------------ strings.po


_ESCAPES = {"n": "\n", "t": "\t", '"': '"', "\\": "\\"}


def unquote(text: str) -> str:
    """A .po string's text: "a \\"b\\"" → a "b"."""
    return re.sub(r"\\(.)", lambda m: _ESCAPES.get(m.group(1), m.group(1)), text)


def parse(text: str, english: bool = False) -> dict:
    """A strings.po's words by id: each msgstr, or for English (the source,
    whose msgstr is empty) its msgid. Words not written yet are left out."""
    found = {}
    entry, field = {}, None

    def done():
        context = re.fullmatch(r"#(\d+)", entry.get("msgctxt", ""))
        if context:
            words = entry.get("msgid" if english else "msgstr", "")
            if words:
                found[int(context.group(1))] = words

    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        match = re.fullmatch(r'(msgctxt|msgid|msgstr)\s+"(.*)"', line)
        if match:
            if match.group(1) == "msgctxt" or (match.group(1) == "msgid" and "msgid" in entry):
                done()
                entry = {}
            field = match.group(1)
            entry[field] = unquote(match.group(2))
        elif line.startswith('"') and line.endswith('"') and field is not None:
            entry[field] += unquote(line[1:-1])
    done()
    return found


_read = {}


def strings(language: str, root: str = ROOT) -> dict:
    """`language`'s words by id, read once."""
    key = (root, language)
    if key not in _read:
        path = os.path.join(root, FOLDERS[language], "strings.po")
        try:
            with open(path, encoding="utf-8") as f:
                _read[key] = parse(f.read(), english=language == ENGLISH)
        except OSError:
            _read[key] = {}
    return _read[key]


class Words:
    """The add-on's words in `language`, English standing in for a word it
    lacks."""

    def __init__(self, language: str, root: str = ROOT):
        self.language = language if language in LANGUAGES else ENGLISH
        self.own = strings(self.language, root)
        self.english = strings(ENGLISH, root)

    def text(self, id_: int) -> str:
        return self.own.get(id_) or self.english.get(id_, "")

    def counted(self, first: int, n) -> str:
        """The words of `n` of something, its plural form chosen: their
        first id is the form "one"."""
        own = self.own.get(first + FORMS.index(plural_form(self.language, n)))
        return own or self.english.get(first + FORMS.index(plural_form(ENGLISH, n)), "")

    def number(self, n) -> str:
        return number(n, self.language)

    def decimal(self, value, digits: int = 1) -> str:
        return decimal(value, digits, self.language)
