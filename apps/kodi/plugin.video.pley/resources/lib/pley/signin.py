"""The sign-in window, as the Android TV app's link screen: where to go,
the code to type there, and the same as a QR code the server draws, while
the plugin asks the server for the answer; and a note when the server
requires two-step sign-in, which the phone sets up."""

from __future__ import annotations

import xbmcgui

from . import layout as L
from .kodi import text

# Back and Menu cancel.
DISMISS = (9, 10, 92)


def halves(code: str) -> tuple:
    """"BCDF-GHJK" as its two fields' text, letters spaced as on a code pad."""
    first, _, second = code.partition("-")
    return " ".join(first), " ".join(second)


def shown(address: str) -> str:
    """The address as people type it: without "http://"."""
    return address.split("://", 1)[-1].rstrip("/")


class SignIn(xbmcgui.WindowXMLDialog):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.address = ""
        self.code = ""
        self.status = ""
        self.qr = ""
        # The server requires two-step sign-in: a note says the phone sets it up.
        self.two_step = False
        self.cancelled = False
        # Change server was chosen: the plugin offers the servers after.
        self.change = False
        self.ready = False

    def fill(self, address: str, code: str, status: str, qr: str = "") -> None:
        self.address, self.code, self.status, self.qr = address, code, status, qr

    def set_status(self, status: str) -> None:
        self.status = status
        if self.ready:
            self.getControl(L.SIGNIN_STATUS).setLabel(status)

    def show_two_step(self) -> None:
        """The server requires two-step sign-in, as it answered on another
        thread, before or after onInit: marked before `ready` is read, so
        one of them shows the note, and neither ever clears it."""
        self.two_step = True
        if self.ready:
            self.getControl(L.SIGNIN_TWO_STEP).setLabel(text("two_step_required"))

    def onInit(self) -> None:
        self.ready = True
        first, second = halves(self.code)
        self.getControl(L.SIGNIN_CODE_A).setLabel(first)
        self.getControl(L.SIGNIN_CODE_B).setLabel(second)
        self.getControl(L.SIGNIN_STATUS).setLabel(self.status)
        if self.two_step:
            self.getControl(L.SIGNIN_TWO_STEP).setLabel(text("two_step_required"))
        # Without a QR code (an older server) the steps are two, and "Go to".
        qr = bool(self.qr)
        self.getControl(L.SIGNIN_ADDRESS).setLabel(text("or_go_to" if qr else "go_to") % shown(self.address))
        self.getControl(L.SIGNIN_STEP_SCAN).setVisible(qr)
        self.getControl(L.SIGNIN_QR_CARD).setVisible(qr)
        if qr:
            self.getControl(L.SIGNIN_QR).setImage(self.qr, False)
        else:
            self.getControl(L.SIGNIN_NUMBER_GO).setLabel("1")
            self.getControl(L.SIGNIN_NUMBER_ENTER).setLabel("2")
            self.getControl(L.SIGNIN_STEP_GO).setPosition(L.GUTTER, 352)
            self.getControl(L.SIGNIN_STEP_ENTER).setPosition(L.GUTTER, 432)
        self.setFocusId(L.SIGNIN_CHANGE)

    def onClick(self, control_id: int) -> None:
        if control_id == L.SIGNIN_CHANGE:
            self.change = True
            self.cancel()

    def onAction(self, action) -> None:
        if action.getId() in DISMISS:
            self.cancel()

    def cancel(self) -> None:
        self.cancelled = True
        self.close()
