"""Stand-ins for Kodi's modules, enough for the add-on's tests."""

LOGDEBUG, LOGINFO, LOGWARNING, LOGERROR = 0, 1, 2, 3
info_labels = {}
conditions = set()
builtins = []
logged = []


def log(message, level=LOGINFO):
    logged.append((level, message))


# Kodi's regional formats (getRegion).
region = {"time": "%H:%M:%S"}


def getRegion(name):
    return region.get(name, "")


def getInfoLabel(name):
    return info_labels.get(name, "")


def getCondVisibility(name):
    return name in conditions


def sleep(ms):
    pass


def executebuiltin(command, wait=False):
    builtins.append(command)


def executeJSONRPC(request):
    return '{"result": {"value": null}}'


PLAYLIST_MUSIC, PLAYLIST_VIDEO = 0, 1
# What Player.play was given: (item, listitem, startpos).
played = []
# What else the player was asked to do: ("pause",), ("seekTime", seconds),
# ("playnext",), ("playprevious",), ("playselected", place).
asked = []


class Player:
    def __init__(self):
        pass

    def play(self, item=None, listitem=None, windowed=False, startpos=-1):
        played.append((item, listitem, startpos))

    def pause(self):
        asked.append(("pause",))

    def seekTime(self, seconds):
        asked.append(("seekTime", seconds))

    def playnext(self):
        asked.append(("playnext",))

    def playprevious(self):
        asked.append(("playprevious",))

    def playselected(self, selected):
        asked.append(("playselected", selected))


class PlayList:
    """Kodi's playlists, one of each kind: what is in them, and whether
    they are shuffled."""

    entries = {}
    shuffled = {}
    # The place of the entry Kodi plays, of each kind (-1: none).
    positions = {}

    def __init__(self, kind):
        self.kind = kind

    def clear(self):
        PlayList.entries[self.kind] = []

    def add(self, url, listitem=None, index=-1):
        PlayList.entries.setdefault(self.kind, []).append((url, listitem))

    def shuffle(self):
        PlayList.shuffled[self.kind] = True

    def unshuffle(self):
        PlayList.shuffled[self.kind] = False

    def getposition(self):
        return PlayList.positions.get(self.kind, -1)

    def size(self):
        return len(PlayList.entries.get(self.kind, []))

    def __getitem__(self, at):
        import xbmcgui

        url, _ = PlayList.entries.get(self.kind, [])[at]
        return xbmcgui.ListItem(path=url)


class Monitor:
    def abortRequested(self):
        return False

    def waitForAbort(self, seconds=0):
        return True


class Keyboard:
    """Kodi's keyboard: each one shown takes the next of `typed` (None:
    cancelled), and is kept in `shown` as (heading, its text at first)."""

    typed = []
    shown = []

    def __init__(self, line="", heading="", hidden=False):
        self.line, self.heading = line, heading
        self.text, self.confirmed = line, False

    def doModal(self, autoclose=0):
        Keyboard.shown.append((self.heading, self.line))
        answer = Keyboard.typed.pop(0)
        self.confirmed = answer is not None
        if self.confirmed:
            self.text = answer

    def isConfirmed(self):
        return self.confirmed

    def getText(self):
        return self.text


class Actor:
    def __init__(self, name="", role="", order=-1, thumbnail=""):
        self.name, self.role, self.order, self.thumbnail = name, role, order, thumbnail
