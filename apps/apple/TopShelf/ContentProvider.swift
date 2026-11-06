import Foundation
import PleyShelf
import TVServices

/// Pley's Top Shelf: the Continue Watching row the app saved last in the
/// app group's container, each title with its picture, its progress, and
/// signed links that open its page or play it.
///
/// The extension holds no sign-in and asks the server nothing: it only
/// reads what the app wrote. With nothing saved, or no container (a build
/// without App Groups), it returns `nil`, and tvOS shows the app's own
/// Top Shelf picture.
///
/// The content is made and handed over in one go, in the completion
/// handler's form of the method: what it returns isn't Sendable, so it
/// can't cross from the async form's executor to its caller, and reading
/// a few small files needs no waiting.
final class ContentProvider: TVTopShelfContentProvider {
    override func loadTopShelfContent(completionHandler: @escaping ((any TVTopShelfContent)?) -> Void) {
        completionHandler(Self.content())
    }

    private static func content() -> (any TVTopShelfContent)? {
        guard let store = ShelfStore.appGroup(), let snapshot = store.read(), !snapshot.entries.isEmpty,
              let key = store.linkKey(create: false)
        else { return nil }
        let items = snapshot.entries.map { entry in
            let item = TVTopShelfSectionedItem(identifier: String(entry.itemId))
            item.title = entry.subtitle.isEmpty ? entry.title : "\(entry.title) · \(entry.subtitle)"
            item.imageShape = entry.shape == .poster ? .poster : .hdtv
            if let name = entry.image, let url = store.imageURL(name) {
                item.setImageURL(url, for: [.screenScale1x, .screenScale2x])
            }
            if let progress = entry.progress { item.playbackProgress = progress }
            item.displayAction = TVTopShelfAction(
                url: ShelfLink(action: .open, itemId: entry.itemId, owner: snapshot.owner).url(key: key))
            item.playAction = TVTopShelfAction(
                url: ShelfLink(action: .play, itemId: entry.itemId, owner: snapshot.owner).url(key: key))
            return item
        }
        let section = TVTopShelfItemCollection(items: items)
        section.title = localized("Continue Watching", in: snapshot.language)
        return TVTopShelfSectionedContent(sections: [section])
    }

    /// `key` in `language`, the one the app showed when it saved the row
    /// (docs/APPLE.md, "Languages"); the system's without one.
    private static func localized(_ key: String.LocalizationValue, in language: String?) -> String {
        let bundle = language.flatMap { Bundle.main.path(forResource: $0, ofType: "lproj") }.flatMap(Bundle.init(path:))
        return String(localized: key, bundle: bundle ?? .main, locale: language.map(Locale.init(identifier:)) ?? .current)
    }
}
