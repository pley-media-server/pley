import os
import re

settings = {}

# The add-on's own strings, as Kodi reads them.
STRINGS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "plugin.video.pley", "resources", "language", "resource.language.en_gb", "strings.po")
_strings = {}


def localized(id):
    if not _strings:
        with open(STRINGS, encoding="utf-8") as f:
            for number, text in re.findall(r'msgctxt "#(\d+)"\nmsgid "(.*)"', f.read()):
                _strings[int(number)] = text.replace('\\"', '"')
    return _strings.get(id, "")


class Addon:
    def __init__(self, id=None):
        pass

    def getSettingString(self, name):
        return str(settings.get(name, ""))

    def getSettingInt(self, name):
        return int(settings.get(name, 0))

    def setSettingString(self, name, value):
        settings[name] = value

    def setSettingInt(self, name, value):
        settings[name] = int(value)

    def getLocalizedString(self, id):
        return localized(id)

    def getAddonInfo(self, name):
        # As Kodi's: the icon is its file's path, in the add-on's folder.
        return {"version": "0.1.0", "profile": settings.get("_profile", "."), "path": ".", "icon": "./resources/icon.png"}[name]
