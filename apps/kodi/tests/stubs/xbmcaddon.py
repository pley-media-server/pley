settings = {}


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

    def getAddonInfo(self, name):
        # As Kodi's: the icon is its file's path, in the add-on's folder.
        return {"version": "0.1.0", "profile": settings.get("_profile", "."), "path": ".", "icon": "./resources/icon.png"}[name]
