import CryptoKit
import Foundation
@testable import PleyShelf
import Testing

private func folder() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("pley-shelf-\(UUID().uuidString)", isDirectory: true)
}

private let jpeg = Data([0xFF, 0xD8, 0xFF, 0xE0]) + Data(repeating: 1, count: 60)
private let png = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) + Data(repeating: 2, count: 60)
private let webp = Data("RIFF".utf8) + Data([0, 0, 0, 0]) + Data("WEBP".utf8) + Data(repeating: 3, count: 40)

struct ShelfSnapshotTests {
    @Test func theOwnerNamesTheServerWithoutItsAddress() {
        let owner = ShelfSnapshot.owner(origin: "http://192.168.1.20:8096", profileId: 3)
        #expect(owner.hasSuffix("-3"))
        #expect(owner.count == 18)
        #expect(!owner.contains("192"))
        #expect(owner == ShelfSnapshot.owner(origin: "http://192.168.1.20:8096", profileId: 3))
        #expect(owner != ShelfSnapshot.owner(origin: "http://192.168.1.21:8096", profileId: 3))
    }

    @Test func itHoldsTenEntriesAtMost() {
        let entries = (1...14).map { ShelfEntry(itemId: $0, title: "T\($0)", kind: .continue) }
        let snapshot = ShelfSnapshot(owner: "o", entries: entries)
        #expect(snapshot.entries.map(\.itemId) == Array(1...10))
    }

    @Test func progressStaysBetweenZeroAndOne() {
        #expect(ShelfEntry(itemId: 1, title: "A", progress: 1.4, kind: .continue).progress == 1)
        #expect(ShelfEntry(itemId: 1, title: "A", progress: -0.2, kind: .continue).progress == 0)
    }
}

struct ShelfStoreTests {
    @Test func aSnapshotIsReadBackWithItsPictures() throws {
        let store = ShelfStore(directory: folder())
        #expect(store.read() == nil)
        let name = try #require(ShelfStore.imageName(path: "/api/items/4/images/thumb?v=1", data: jpeg))
        #expect(name.hasSuffix(".jpg"))
        let snapshot = ShelfSnapshot(owner: "abc-1", entries: [
            ShelfEntry(itemId: 4, title: "Show", subtitle: "S1 · E2", progress: 0.4, kind: .continue, image: name),
            ShelfEntry(itemId: 5, title: "Movie", kind: .next),
        ])
        try store.write(snapshot, images: [name: jpeg])
        #expect(store.read() == snapshot)
        #expect(store.hasImage(name))
        #expect(store.savedImage(for: "/api/items/4/images/thumb?v=1") == name)
        #expect(store.savedImage(for: "/api/items/4/images/thumb?v=2") == nil)
    }

    @Test func picturesNoLongerNamedGoAndMissingOnesArentNamed() throws {
        let store = ShelfStore(directory: folder())
        let first = try #require(ShelfStore.imageName(path: "/a", data: png))
        let second = try #require(ShelfStore.imageName(path: "/b", data: webp))
        #expect(second.hasSuffix(".webp"))
        try store.write(ShelfSnapshot(owner: "o", entries: [ShelfEntry(itemId: 1, title: "A", kind: .continue,
                                                                        image: first)]),
                        images: [first: png])
        // The second picture never came: the entry has none.
        try store.write(ShelfSnapshot(owner: "o", entries: [ShelfEntry(itemId: 2, title: "B", kind: .continue,
                                                                        image: second)]))
        #expect(!store.hasImage(first))
        #expect(store.read()?.entries.first?.image == nil)
    }

    @Test func onlyJPEGPNGAndWebPWithinTheLimitAreKept() {
        #expect(ShelfStore.imageName(path: "/a", data: Data("<svg/>".utf8)) == nil)
        #expect(ShelfStore.imageName(path: "/a", data: Data("GIF89a".utf8)) == nil)
        let large = Data([0xFF, 0xD8, 0xFF]) + Data(count: ShelfStore.maxImageBytes)
        #expect(ShelfStore.imageName(path: "/a", data: large) == nil)
        #expect(ShelfStore(directory: folder()).imageURL("../snapshot.json") == nil)
        #expect(ShelfStore(directory: folder()).imageURL("0123456789abcdef.png") != nil)
    }

    @Test func aSnapshotTooLargeIsntRead() throws {
        let directory = folder()
        let store = ShelfStore(directory: directory)
        try store.write(ShelfSnapshot(owner: "o", entries: []))
        let file = directory.appendingPathComponent("snapshot.json")
        try Data(repeating: 0x20, count: ShelfStore.maxSnapshotBytes + 1).write(to: file)
        #expect(store.read() == nil)
    }

    @Test func clearingKeepsTheLinkKey() throws {
        let store = ShelfStore(directory: folder())
        #expect(store.linkKey(create: false) == nil)
        let key = try #require(store.linkKey(create: true))
        let name = try #require(ShelfStore.imageName(path: "/a", data: jpeg))
        try store.write(ShelfSnapshot(owner: "o", entries: [ShelfEntry(itemId: 1, title: "A", kind: .continue,
                                                                        image: name)]),
                        images: [name: jpeg])
        store.clear()
        #expect(store.read() == nil)
        #expect(!store.hasImage(name))
        let again = try #require(store.linkKey(create: false))
        #expect(again.withUnsafeBytes { Data($0) } == key.withUnsafeBytes { Data($0) })
    }

    @Test func withoutAnAppGroupThereIsNoStore() {
        // The test bundle names no group.
        #expect(ShelfStore.appGroup(bundle: Bundle(for: Marker.self)) == nil)
    }
}

private final class Marker {}

struct ShelfLinkTests {
    let key = SymmetricKey(size: .bits256)

    @Test func aSignedLinkIsVerified() throws {
        let link = ShelfLink(action: .play, itemId: 42, owner: "0123456789abcdef-3")
        let url = link.url(key: key)
        #expect(url.absoluteString.hasPrefix("app.pley.apple:/shelf/play/42?owner=0123456789abcdef-3&sig="))
        #expect(ShelfLink.verified(url, key: key) == link)
    }

    @Test func anotherKeyOrAChangedLinkIsRefused() throws {
        let url = ShelfLink(action: .open, itemId: 7, owner: "o-1").url(key: key)
        #expect(ShelfLink.verified(url, key: SymmetricKey(size: .bits256)) == nil)
        let text = url.absoluteString
        for changed in [text.replacingOccurrences(of: "/open/", with: "/play/"),
                        text.replacingOccurrences(of: "/7?", with: "/8?"),
                        text.replacingOccurrences(of: "owner=o-1", with: "owner=o-2")] {
            #expect(ShelfLink.verified(try #require(URL(string: changed)), key: key) == nil)
        }
        let unsigned = try #require(URL(string: "app.pley.apple:/shelf/open/7?owner=o-1"))
        #expect(ShelfLink.verified(unsigned, key: key) == nil)
        #expect(ShelfLink.parse(unsigned) == ShelfLink(action: .open, itemId: 7, owner: "o-1"))
    }

    @Test func otherLinksArentShelfLinks() throws {
        for text in ["https://example.com/shelf/open/7?owner=o", "app.pley.apple:/shelf/delete/7?owner=o",
                     "app.pley.apple:/shelf/open/x?owner=o", "app.pley.apple:/shelf/open/7",
                     "app.pley.apple:/shelf/open/-1?owner=o", "app.pley.apple:/other/open/7?owner=o",
                     "app.pley.apple:/shelf/open/7?owner=o&sig=zz"] {
            let url = try #require(URL(string: text))
            #expect(ShelfLink.verified(url, key: key) == nil, "\(text)")
        }
        let long = "app.pley.apple:/shelf/open/7?owner=" + String(repeating: "a", count: 600)
        #expect(ShelfLink.parse(try #require(URL(string: long))) == nil)
    }
}
