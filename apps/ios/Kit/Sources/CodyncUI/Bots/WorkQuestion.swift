import CodyncKit
import SwiftUI

/// What Stop or Cut in asks about before it acts: an item of `bot`'s, on `store`'s computer.
struct WorkQuestion: Identifiable {
    enum Kind { case stop, cutIn }

    let kind: Kind
    let item: WorkItem
    let bot: Bot
    let store: BotStore
    var id: String { item.id }
}

extension BotStore {
    /// Does `action` on `item` of `bot`'s; Stop and Cut in ask first (`ask`, answered by `workQuestion`).
    func run(_ action: WorkAction, on item: WorkItem, of bot: Bot, ask: (WorkQuestion) -> Void) {
        switch action {
        case .stop: ask(WorkQuestion(kind: .stop, item: item, bot: bot, store: self))
        case .cutIn: ask(WorkQuestion(kind: .cutIn, item: item, bot: bot, store: self))
        case .first: workFirst(bot.id, item: item.id)
        case .drop: dropWork(bot.id, item: item.id)
        case .retry: retryWork(bot.id, item: item.id)
        }
    }
}

extension View {
    /// Stop's and Cut in's dialogs for `question`.
    func workQuestion(_ question: Binding<WorkQuestion?>) -> some View {
        let asked = question.wrappedValue
        return stopWorkDialog(isPresented: shown(question, .stop)) {
            if let asked { asked.store.stopWork(asked.bot.id, item: asked.item.id) }
        }
        .codyncDialog(L10n.fill(L10n.tr("Let \"{title}\" cut in"), ["title": asked?.item.title ?? ""]), isPresented: shown(question, .cutIn),
                      message: "Every place is taken. Pick what gives way: its session closes and it starts again later.") {
            (asked.map { WorkSections($0.bot.work).givingWay } ?? []).map { giving in
                DialogAction(cutInChoice(giving)) {
                    if let asked { asked.store.workCutIn(asked.bot.id, item: asked.item.id, insteadOf: giving.id) }
                }
            }
        }
    }
}

private func shown(_ question: Binding<WorkQuestion?>, _ kind: WorkQuestion.Kind) -> Binding<Bool> {
    Binding(get: { question.wrappedValue?.kind == kind }, set: { if !$0 { question.wrappedValue = nil } })
}

/// A working item in the cut-in dialog: its title, what it does, how long.
private func cutInChoice(_ item: WorkItem) -> String {
    let elapsed = item.startedAt.map { Duration.seconds(max(0, Date.now.timeIntervalSince(Date(milliseconds: $0)))).formatted(.time(pattern: .minuteSecond)) }
    return [item.title, item.status == .needsYou || !item.activity.isEmpty ? item.what() : nil, elapsed]
        .compactMap(\.self).joined(separator: " · ")
}
