"""A library's collections by category, and their Category filter, as
the web's Collections tab has them: the categories in the order
administrators chose (GET /api/collection-categories), Pley's Other
among them where it is, empty groups left out. The server sends Other as
the category of a collection without one. Pley's window and Kodi's lists
both show them; managing categories is on the web."""

from __future__ import annotations

# The Category filter's choice besides a category's id.
ALL = "all"
# Pley's Other: the category the server sends for a collection without one.
OTHER_KEY = "other"


def groups(collections: list, categories: list) -> list:
    """(category, its collections) in the categories' order; one of a
    category not listed yet (made since) comes after the rest, under its
    own. Empty groups left out."""
    out = {category["id"]: (category, []) for category in categories}
    for collection in collections:
        category = collection["category"]
        out.setdefault(category["id"], (category, []))[1].append(collection)
    return [(category, found) for category, found in out.values() if found]


def key(category) -> object:
    """A group's choice in the Category filter: its id."""
    return category["id"]


def chosen(choice, every: list) -> object:
    """The Category filter as it applies: ALL when the category chosen is
    no longer among the groups (deleted, emptied)."""
    if choice == ALL or any(key(category) == choice for category, _ in every):
        return choice
    return ALL


def shown(collections: list, categories: list, choice) -> list:
    """The groups shown: the category chosen's, or every one's."""
    found = groups(collections, categories)
    if choice == ALL:
        return found
    return [(category, of) for category, of in found if key(category) == choice]


def title(category) -> str:
    """A group's heading: the category's name, in the person's language."""
    return category["title"]


def said(collection: dict):
    """The category a collection's page names: none for Other, which only
    says it has none."""
    category = collection.get("category") or {}
    return None if category.get("key") == OTHER_KEY else category.get("title")


def parse(value) -> object:
    """A Category choice from a listing's address: ALL or an id."""
    if value in (None, "", ALL):
        return ALL
    try:
        return int(value)
    except (TypeError, ValueError):
        return ALL
