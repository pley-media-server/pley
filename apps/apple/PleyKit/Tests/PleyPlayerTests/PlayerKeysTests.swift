import PleyPlayer
import Testing

@Suite struct PlayerKeysTests {
    @Test func theWebPlayersKeys() {
        #expect(PlayerKeys.action(" ") == .playPause)
        #expect(PlayerKeys.action("k") == .playPause)
        #expect(PlayerKeys.action("K") == .playPause)
        #expect(PlayerKeys.action("j") == .seek(byMs: -10_000))
        #expect(PlayerKeys.action(String(PlayerKeys.Special.left)) == .seek(byMs: -10_000))
        #expect(PlayerKeys.action("l") == .seek(byMs: 10_000))
        #expect(PlayerKeys.action(String(PlayerKeys.Special.right)) == .seek(byMs: 10_000))
        #expect(PlayerKeys.action(String(PlayerKeys.Special.up)) == .volume(by: 0.05))
        #expect(PlayerKeys.action(String(PlayerKeys.Special.down)) == .volume(by: -0.05))
        #expect(PlayerKeys.action("m") == .mute)
        #expect(PlayerKeys.action("f") == .fullScreen)
        #expect(PlayerKeys.action(String(PlayerKeys.Special.escape)) == .leaveFullScreen)
        #expect(PlayerKeys.action("s") == .skip)
    }

    @Test func otherKeysAndMenuShortcutsAreLeftAlone() {
        #expect(PlayerKeys.action("x") == nil)
        #expect(PlayerKeys.action("") == nil)
        #expect(PlayerKeys.action("kk") == nil)
        #expect(PlayerKeys.action("f", command: true) == nil)
        #expect(PlayerKeys.action(" ", command: true) == nil)
    }
}
