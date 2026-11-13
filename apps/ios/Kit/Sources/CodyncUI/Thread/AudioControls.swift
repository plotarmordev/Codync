import CodyncKit
import SwiftUI

/// A shared sound's play/pause: a spinner while it loads, and the 44 pt tap target of every card-sized control.
/// The player row shows it; a plain glyph, no tile in any state.
struct AudioPlayButton: View {
    let state: SharedAudio.State
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Group {
                if state == .loading {
                    Spinner(size: 24)
                } else {
                    Image(systemName: state == .playing ? "pause.circle" : "play.circle")
                        .font(.system(size: 24)).foregroundStyle(Palette.text)
                        .contentTransition(.symbolEffect(.replace))
                }
            }
            .frame(width: 44, height: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// The player row's trailing download, drawn like its play button: a plain glyph, no tile in any state.
struct AudioDownloadButton: View {
    let downloading: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: downloading ? "xmark" : "arrow.down.to.line")
                .font(.system(size: 24)).foregroundStyle(Palette.text)
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// How far a shared sound has played: a 4 pt bar filling left to right, always shown — empty and inert until the
/// sound's length is known (the loaded sound, or the computer's stamp). Tap or drag it to move there; its touch
/// area runs 20 pt over and under the bar without taking up layout, as the play button's does.
struct AudioProgress: View {
    let elapsed: TimeInterval
    let duration: TimeInterval
    let seek: (TimeInterval) -> Void

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(Palette.tertiary)
                Capsule().fill(Palette.text).frame(width: proxy.size.width * min(1, elapsed / max(1, duration)))
            }
            .frame(height: 4)
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                seek(duration * min(max(drag.location.x / proxy.size.width, 0), 1))
            })
        }
        .frame(height: 44)
        .padding(.vertical, -20)
    }
}

extension View {
    /// VoiceOver's swipe up or down on a voice note moves it five seconds; moving needs the loaded sound, as on
    /// the bar.
    func audioSeekAdjustable(_ audio: SharedAudio) -> some View {
        accessibilityAdjustableAction { direction in
            audio.seek(to: audio.elapsed + (direction == .increment ? 5 : -5))
        }
    }
}
