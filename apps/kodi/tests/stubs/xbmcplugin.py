SORT_METHOD_UNSORTED, SORT_METHOD_LABEL, SORT_METHOD_VIDEO_YEAR = 0, 1, 2
calls = []
# Sorts added: (method, labelMask, label2Mask).
sorts = []


def addDirectoryItem(handle, url, listitem, isFolder=False, totalItems=0):
    calls.append(("item", url, listitem, isFolder))
    return True


def addDirectoryItems(handle, items, totalItems=0):
    for url, listitem, folder in items:
        calls.append(("item", url, listitem, folder))
    return True


def endOfDirectory(handle, succeeded=True, updateListing=False, cacheToDisc=True):
    calls.append(("end", succeeded))


def setResolvedUrl(handle, succeeded, listitem):
    calls.append(("resolved", succeeded, listitem))


def setContent(handle, content):
    calls.append(("content", content))


def setPluginCategory(handle, category):
    calls.append(("category", category))


def addSortMethod(handle, sortMethod, labelMask="", label2Mask=""):
    sorts.append((sortMethod, labelMask, label2Mask))
