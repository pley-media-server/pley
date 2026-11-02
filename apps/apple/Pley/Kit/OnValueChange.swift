import SwiftUI

extension View {
    /// `onChange(of:)` with the new value, in the form each system has: the
    /// two-value form on the Mac and from iOS 17, the one-value form, which
    /// iOS 16 has only, before.
    @ViewBuilder
    func onValueChange<V: Equatable>(of value: V, perform action: @escaping (V) -> Void) -> some View {
        #if os(macOS)
        onChange(of: value) { _, new in action(new) }
        #else
        if #available(iOS 17, *) {
            onChange(of: value) { _, new in action(new) }
        } else {
            onChange(of: value, perform: action)
        }
        #endif
    }
}
