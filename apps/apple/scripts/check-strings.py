#!/usr/bin/env python3
"""Checks the Apple apps' String Catalogs (docs/APPLE.md, "Languages").

Every Localizable.xcstrings under apps/apple (the app targets, Top Shelf
and each PleyKit module) must have, for every key:

- a French and a Spanish translation, marked translated;
- the plural forms French and Spanish need (one, many, other) wherever a
  count varies the words;
- the same placeholders (%@, %lld, %1$@ ...) as the English;
- no stale entry: one Xcode no longer finds in the code.

With --derived-data, the strings the compiler found in the code of builds
(their .stringsdata files) are checked too: each must be in its catalog, and
each catalog key must be found in the code, so a word added or changed
without its translation fails, and so does one left behind. Give the
iPhone, Apple TV and Mac builds' folders together: each compiles only its
own platform's code.

    python3 apps/apple/scripts/check-strings.py
    python3 apps/apple/scripts/check-strings.py --derived-data /tmp/dd-ios /tmp/dd-tv /tmp/dd-mac

Exits 1 and lists every problem when there is one.
"""

import argparse
import json
import re
import sys
from pathlib import Path

APPLE = Path(__file__).resolve().parent.parent
LANGUAGES = ("fr", "es")
PLURAL_FORMS = ("one", "many", "other")

# A printf placeholder as String(localized:) writes them, `%%` aside.
PLACEHOLDER = re.compile(r"%(?:(\d+)\$)?[-+ #0']*\d*(?:\.\d+)?(hh|h|ll|l|q|z|t|j|L)?([@dDuUxXoOfFeEgGcCsSpaA])")


def catalogs(root=APPLE):
    """Each catalog's owner (Pley, tvOS, TopShelf or a PleyKit module) and path."""
    found = {}
    for path in sorted(root.glob("*/Localizable.xcstrings")):
        found[path.parent.name] = path
    for path in sorted((root / "PleyKit" / "Sources").glob("*/Localizable.xcstrings")):
        found[path.parent.name] = path
    return found


def placeholders(text):
    """The placeholders of `text` by position: {1: "lld", 2: "@"}."""
    out, next_position = {}, 1
    for match in PLACEHOLDER.finditer(text.replace("%%", "")):
        position, length, conversion = match.groups()
        if position:
            index = int(position)
        else:
            index = next_position
            next_position += 1
        out[index] = (length or "") + conversion
    return out


def units(localization):
    """A localization's strings, by plural form ("" when it has none), or None for another kind of variation."""
    if "stringUnit" in localization:
        return {"": localization["stringUnit"]}
    variations = localization.get("variations", {})
    if "plural" in variations:
        return {form: value.get("stringUnit", {}) for form, value in variations["plural"].items()}
    if variations:
        # Device variations (iPhone, Apple TV ...): each one's own string.
        found = {}
        for kind in variations.values():
            for name, value in kind.items():
                found[name] = value.get("stringUnit", {})
        return found
    return {}


def check_entry(key, entry, where):
    """The problems of one catalog entry, as lines."""
    problems = []
    if entry.get("shouldTranslate") is False:
        return problems
    if entry.get("extractionState") == "stale":
        problems.append(f"{where}: {key!r} is stale: the code no longer has it")
    localizations = entry.get("localizations", {})
    english = units(localizations.get("en", {})) or {"": {"value": key}}
    english_plural = set(english) - {""}
    # The English a translation's placeholders are compared with: its "other" form, or the key itself.
    english_text = (english.get("other") or english.get("") or {}).get("value", key)
    english_places = placeholders(english_text)
    if placeholders(key) != english_places and "" in english:
        problems.append(f"{where}: {key!r}: the English value's placeholders differ from the key's")
    for language in LANGUAGES:
        found = localizations.get(language)
        if not found:
            problems.append(f"{where}: {key!r} has no {language} translation")
            continue
        forms = units(found)
        if english_plural and not ({"one", "other"} <= set(forms)):
            problems.append(f"{where}: {key!r}: {language} has no plural forms, as the English does")
        if set(forms) & set(PLURAL_FORMS):
            missing = [form for form in PLURAL_FORMS if form not in forms]
            if missing:
                problems.append(f"{where}: {key!r}: {language} lacks the plural form(s) {', '.join(missing)}")
        for form, unit in forms.items():
            label = f"{language}{'/' + form if form else ''}"
            if unit.get("state") not in ("translated", None) or "value" not in unit:
                problems.append(f"{where}: {key!r}: {label} isn't translated (state {unit.get('state')!r})")
                continue
            value = unit["value"]
            if not value.strip():
                problems.append(f"{where}: {key!r}: {label} is empty")
            got = placeholders(value)
            # "one" may leave the count out ("Un élément"), never add or change one.
            if form in ("one", "zero", "two") and all(english_places.get(i) == kind for i, kind in got.items()):
                continue
            if got != english_places:
                problems.append(f"{where}: {key!r}: {label} has placeholders {sorted(got.items())}, "
                                f"the English {sorted(english_places.items())}")
    return problems


def check_catalog(path, name=None):
    """The problems of one catalog file."""
    where = name or str(path)
    try:
        catalog = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        return [f"{where}: can't be read: {error}"]
    problems = []
    if catalog.get("sourceLanguage") != "en":
        problems.append(f"{where}: the source language isn't en")
    for key, entry in sorted(catalog.get("strings", {}).items()):
        problems.extend(check_entry(key, entry, where))
    return problems


def extracted(folders):
    """The keys the compiler found in each owner's code: {owner: {key: [file:line]}}."""
    found = {}
    datas = (data for folder in folders for data in Path(folder).rglob("*.stringsdata"))
    for data in datas:
        try:
            content = json.loads(data.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        source = Path(content.get("source", ""))
        try:
            relative = source.resolve().relative_to(APPLE)
        except ValueError:
            continue
        parts = relative.parts
        if parts[0] == "PleyKit" and len(parts) > 2 and parts[1] == "Sources":
            owner = parts[2]
        elif parts[0] in ("Pley", "tvOS", "TopShelf"):
            owner = parts[0]
        else:
            continue
        for table, entries in content.get("tables", {}).items():
            if table != "Localizable":
                continue
            for entry in entries:
                line = entry.get("location", {}).get("startingLine", 0)
                found.setdefault(owner, {}).setdefault(entry["key"], []).append(f"{relative}:{line}")
    return found


def check_code(folders, owned):
    """Keys the code uses that its catalog lacks, and catalog keys no code uses."""
    problems = []
    used = extracted(folders)
    if not used:
        return [f"{', '.join(folders)}: no .stringsdata: build with this derived data first"]
    for owner, keys in sorted(used.items()):
        path = owned.get(owner)
        have = set(json.loads(path.read_text(encoding="utf-8")).get("strings", {})) if path else set()
        for key, places in sorted(keys.items()):
            # Placeholders and punctuation alone ("%@ · %@") have no words to translate.
            if not re.search(r"[^\W\d_]", PLACEHOLDER.sub("", key)):
                continue
            if key not in have:
                problems.append(f"{owner}: {key!r} ({places[0]}) isn't in {owner}'s catalog")
    for owner, path in sorted(owned.items()):
        catalog = json.loads(path.read_text(encoding="utf-8")).get("strings", {})
        for key, entry in sorted(catalog.items()):
            if entry.get("extractionState") == "manual":
                continue
            if key not in used.get(owner, {}):
                problems.append(f"{owner}: {key!r} is in the catalog but not in the code")
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--derived-data", nargs="+", metavar="FOLDER",
                        help="builds' derived data, to compare with the code's strings")
    args = parser.parse_args()
    owned = catalogs()
    problems = []
    for name, path in owned.items():
        problems.extend(check_catalog(path, name))
    if args.derived_data:
        problems.extend(check_code(args.derived_data, owned))
    for problem in problems:
        print(problem)
    keys = sum(len(json.loads(p.read_text(encoding="utf-8")).get("strings", {})) for p in owned.values())
    print(f"{len(owned)} catalogs, {keys} keys: {'%d problems' % len(problems) if problems else 'all translated'}",
          file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
