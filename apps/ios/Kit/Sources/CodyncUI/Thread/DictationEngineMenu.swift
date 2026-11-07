import CodyncKit
import SwiftUI

/// Beside the composer's mic, in its capsule: which speech dictation uses — on-device, a provider on
/// user's own key, or the speech server the computer is linked to.
struct DictationEngineMenu: View {
    let height: CGFloat
    @AppStorage(VoiceSettings.dictationEngineKey) private var engine = "device"

    var body: some View {
        DropdownMenu {
            [MenuItem("On device", selected: engine == "device") { engine = "device" }]
                + VoiceProvider.allCases.map { provider in
                    MenuItem(provider.name, selected: engine == provider.rawValue) { engine = provider.rawValue }
                }
                + [MenuItem(DictationEngine.local.name, selected: engine == DictationEngine.local.pref) {
                    engine = DictationEngine.local.pref
                }]
        } label: {
            Image(systemName: "chevron.up")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(Palette.secondary)
                .frame(width: 14, height: height)
                // The capsule's end is part of the tap area, away from the mic.
                .padding(.trailing, 6)
                .contentShape(Rectangle())
        }
        .accessibilityLabel("Dictation engine")
        .help("Dictation engine")
    }
}
