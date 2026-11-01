import unittest

import support  # noqa: F401
from pley import layout as L
from pley import signin

TWO_STEP = "This server requires two-step sign-in: your phone sets it up if you haven't yet."


class Control:
    """A control that remembers what it was given; empty at first, as the
    skin has the ones signin.py fills."""

    def __init__(self):
        self.label = ""
        self.visible = True
        self.position = None
        self.image = None

    def setLabel(self, text):
        self.label = text

    def setVisible(self, shown):
        self.visible = shown

    def setPosition(self, x, y):
        self.position = (x, y)

    def setImage(self, path, use_cache=True):
        self.image = path


class Screen(signin.SignIn):
    """The sign-in window, on controls that only remember."""

    def __init__(self):
        super().__init__()
        self.made = {}
        self.focused = None

    def getControl(self, control_id):
        return self.made.setdefault(control_id, Control())

    def setFocusId(self, control_id):
        self.focused = control_id


def screen() -> Screen:
    shown = Screen()
    shown.fill("http://192.168.1.5:7539/link", "BCDF-GHJK", "Waiting for approval", "pley-qr-BCDFGHJK.png")
    return shown


class SignInTest(unittest.TestCase):
    """Signing in with a code (TvLinkScreen), and what it says about two-step
    sign-in on a server that requires it."""

    def test_it_says_so_when_the_server_requires_two_step_sign_in(self):
        # The server answers once the screen shows...
        late = screen()
        late.onInit()
        self.assertEqual(late.getControl(L.SIGNIN_TWO_STEP).label, "")
        late.show_two_step()
        self.assertEqual(late.made[L.SIGNIN_TWO_STEP].label, TWO_STEP)
        # ...or before Kodi drew it: kept until then.
        early = screen()
        early.show_two_step()
        self.assertEqual(early.made, {})
        early.onInit()
        self.assertEqual(early.made[L.SIGNIN_TWO_STEP].label, TWO_STEP)
        self.assertEqual((early.made[L.SIGNIN_CODE_A].label, early.made[L.SIGNIN_CODE_B].label), ("B C D F", "G H J K"))

    def test_it_says_nothing_of_it_otherwise(self):
        # A server that doesn't require it, or doesn't say: the note stays empty.
        shown = screen()
        shown.onInit()
        shown.set_status("Waiting for approval")
        self.assertEqual(shown.getControl(L.SIGNIN_TWO_STEP).label, "")
        self.assertEqual(shown.made[L.SIGNIN_ADDRESS].label, "or go to [B]192.168.1.5:7539/link[/B]")
        self.assertEqual(shown.focused, L.SIGNIN_CHANGE)


if __name__ == "__main__":
    unittest.main()
