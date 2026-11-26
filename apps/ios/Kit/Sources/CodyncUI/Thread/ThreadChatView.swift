import CodyncKit
import SwiftUI

extension EnvironmentValues {
    /// The item's thread the views under it write to: the composer, its waiting messages and the user's bubbles
    /// address it instead of the bot's chat. Nil in a chat.
    @Entry var workThread: WorkThread?
}

/// What a pushed thread chat shows: an item of a bot's (or of a computer's box without a bot), from the Threads tab.
public struct ThreadTarget {
    public var of: RosterItem
    public var item: WorkItem

    public init(of: RosterItem, item: WorkItem) {
        self.of = of
        self.item = item
    }
}

public extension View {
    /// Pushes `thread` over this view in the chat's form (`ThreadChatView`), inside its navigation stack, with the tab
    /// bar hidden; `store` is its computer's. Put it outside what the stack swaps (a list and its empty states), so it
    /// pushes whichever shows.
    func threadChat(_ thread: Binding<ThreadTarget?>, store: @escaping (RosterItem) -> BotStore?) -> some View {
        navigationDestination(isPresented: Binding(get: { thread.wrappedValue != nil }, set: { if !$0 { thread.wrappedValue = nil } })) {
            if let target = thread.wrappedValue, let store = store(target.of) {
                ThreadChatView(botId: target.of.bot.id, item: target.item)
                    .environment(store)
                    .toolbar(.hidden, for: .tabBar)
            }
        }
    }
}

/// An item's thread in the chat's own form, the desktop's `ThreadChat`: the user's messages and the thread's replies as
/// the chat's bubbles, what it put in the bot's chat (a card to answer, a file, an artifact) as the chat's own rows,
/// what it was told folded on top, and while it works the chat's working line with its steps. On top: Back, the bot
/// (a thread without a bot: its agent) and the title over how it stands, and Details for the bot's details. Under it:
/// the chat's composer writing to the thread, with the agent it runs on and where it works; for a routine's run or
/// another bot's request, why it is read only, and Stop while it runs.
struct ThreadChatView: View {
    let botId: String
    /// The item as its row had it; the bot's own copy is shown while it carries one.
    let item: WorkItem
    /// Details opens the bot's details over it; off where it is pushed inside those details.
    var showsDetails = true
    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    /// The thread's transcript as the computer has it; nil while it loads.
    @State private var fetched: [Entry]?
    @State private var error: String?
    @State private var attempt = 0
    @State private var confirmingStop = false
    @State private var following = true
    @State private var toldOpen = false
    @State private var details: DetailsRequest?
    @State private var artifact: ArtifactTarget?
    @State private var botChat: BotChatTarget?
    @State private var viewingImage: ImageViewing?

    /// What reads the transcript again: Try again, a task taking the item, the task ending, and the link coming back
    /// (its catch-up carries no task entries).
    private struct Load: Hashable {
        var attempt: Int
        var task: String?
        var live: Bool
        var online: Bool
    }

    var body: some View {
        let item = model.bots[botId]?.work.first { $0.id == self.item.id } ?? self.item
        let live = item.status.isLive
        let entries = (fetched ?? []).mergingTaskEntries(item.task.flatMap { model.taskEntries[$0] } ?? [])
        let transcript = entries.threadEntries
        ConversationList(rows: rows(transcript, steps: Entry.workingSteps(entries), of: item), following: $following)
            .background(Palette.background)
            .safeAreaInset(edge: .top, spacing: 0) { header(item) }
            .safeAreaInset(edge: .bottom, spacing: 0) { footer(item, queued: transcript.queue(working: live)) }
            .environment(\.workThread, item.thread(of: botId))
            .hidesSystemNavigationBar()
            .task(id: Load(attempt: attempt, task: item.task, live: live, online: model.connection == .online)) { await load(item.task) }
            // The card it waits on is the bot's chat's, maybe further back than the pages loaded: read back to it.
            .task(id: item.status == .needsYou) { if item.status == .needsYou { _ = await model.card(for: item, in: botId) } }
            .stopWorkDialog(isPresented: $confirmingStop) { model.stopWork(botId, item: item.id) }
            .imageViewer($viewingImage)
            .fileExportSheet(model, botId: botId, active: details == nil && viewingImage == nil)
            .codyncSheet(item: $artifact) { ArtifactView(target: $0) }
            .codyncSheet(item: $botChat) { BotConversationView(botId: botId, peerId: $0.id) }
            .codyncSheet(item: $details) { request in
                BotDetailsView(botId: botId, request: request) { picked in
                    details = nil
                    artifact = picked
                }
            }
            .tracked(.screenViewed(.task))
    }

    private var bot: Bot? { model.bots[botId] }

    /// Back, the bot (a thread without a bot: its agent) and the title over how it stands, then Details.
    private func header(_ item: WorkItem) -> some View {
        HStack(spacing: 8) {
            IconButton("Back", systemImage: "chevron.left") { dismiss() }
            if let bot {
                if bot.plain {
                    AgentIcon(registry: model.agentRegistry(item.agent ?? bot.backend), size: 18)
                } else {
                    CharacterAvatar(bot: bot, size: 22, animated: false)
                }
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                    .accessibilityAddTraits(.isHeader)
                // Where it works sits in the composer's row; a thread without one says it here.
                ThreadStatusLine(item: item, showsWhere: !takesMessages(item))
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if showsDetails, let bot, !bot.plain {
                IconButton("Details", systemImage: "sidebar.right") { details = DetailsRequest() }
            }
        }
        .padding(.leading, 10)
        .padding(.trailing, 12)
        .padding(.top, 12)
        .padding(.bottom, 6)
        .background(Palette.background)
    }

    private func takesMessages(_ item: WorkItem) -> Bool { item.talk && item.task != nil }

    /// The thread's chat rows (`ChatItem.thread`) with the state above them and the working line under them.
    private func rows(_ transcript: ThreadEntries, steps: [Entry], of item: WorkItem) -> [ConversationRow] {
        let live = item.status.isLive
        let items = ChatItem.thread(transcript, chat: model.chat(botId), task: item.task, working: live)
        var rows: [ConversationRow] = []
        if error != nil || fetched == nil || items.isEmpty {
            rows.append(ConversationRow("state") { state.frame(maxWidth: .infinity).padding(.vertical, 14) })
        }
        rows += items.map { item in
            ConversationRow(item.id, isUserMessage: item.isUserMessage && !item.isToldToTask) { row(item, told: transcript.told) }
        }
        // Offline, "working" is only what the computer last said; don't show it as live.
        if live, !model.isOffline, let bot {
            rows.append(ConversationRow("working") {
                WorkingIndicator(bot: working(bot, item), steps: steps)
                    .id(item.startedAt)
                    .padding(.top, 6)
            })
        }
        return rows
    }

    /// The bot as the working line reads it, with the task's own activity and time.
    private func working(_ bot: Bot, _ item: WorkItem) -> Bot {
        var bot = bot
        bot.status = item.status == .needsYou ? "needsInput" : "working"
        bot.activity = item.activity
        bot.startedAt = item.startedAt
        return bot
    }

    @ViewBuilder private func row(_ item: ChatItem, told: Entry?) -> some View {
        switch item.kind {
        case .separator(let date):
            TimeSeparator(date: date)
        case .exchanges(let group):
            BotMessageRow(group: group) { botChat = BotChatTarget(id: group.peerId) }
        case .requests(let entries):
            RequestRun(entries: entries) { e in
                PermissionCard(entry: e, hostName: model.hostName, answering: model.answering[e.id], respond: { model.respond(e, option: $0) }, showsEnding: false)
            }
            .padding(.top, 12)
        case .entry(let e, let groupStart):
            if e.id == told?.id {
                ToldFold(entry: e, isExpanded: $toldOpen)
            } else if e.isToldToTask, e.botId != botId {
                // Told again later (an item picked up): a quoted block where it happened, not something the user typed.
                ToldText(entry: e)
                    .padding(.leading, 12)
                    .overlay(alignment: .leading) { Capsule().fill(Palette.tertiary).frame(width: 2) }
                    .padding(.vertical, 8)
            } else {
                // What the thread put in the bot's chat is that chat's entry, answered and opened here as there.
                ChatRow(entry: e, groupStart: groupStart, chat: bot, openTrace: {},
                        openArtifact: { artifact = ArtifactTarget(botId: botId, id: $0.id, title: $0.title) })
            }
        }
    }

    @ViewBuilder private var state: some View {
        if let error {
            VStack(spacing: 8) {
                Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger)
                Button(L10n.tr("Try again")) { attempt += 1 }
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Palette.text)
                    .buttonStyle(PressScale())
            }
        } else if fetched == nil {
            Spinner(size: 14).accessibilityLabel(L10n.tr("Loading"))
        } else {
            Text(L10n.tr("Nothing yet.")).font(.footnote).foregroundStyle(Palette.tertiary)
        }
    }

    /// The composer for an item that takes messages (its Stop stops the thread, asking first); for a routine's run or
    /// another bot's request, why it doesn't, with Stop while it runs.
    @ViewBuilder private func footer(_ item: WorkItem, queued: [Entry]) -> some View {
        if takesMessages(item) {
            Composer(botId: botId, replyingTo: .constant(nil), queued: queued, stopThread: { confirmingStop = true })
                .disabled(model.isOffline)
        } else if let line = item.readOnlyLine {
            HStack(spacing: 8) {
                Text(L10n.tr(line)).font(.footnote).foregroundStyle(Palette.tertiary)
                if item.status.isLive {
                    IconButton("Stop", systemImage: "stop.fill") { confirmingStop = true }
                        .disabled(model.isOffline)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 10)
            .background(Palette.background)
        }
    }

    private func load(_ task: String?) async {
        error = nil
        guard let task else {
            fetched = []
            return
        }
        do {
            fetched = try await model.fetchTaskEntries(botId, task: task)
        } catch is CancellationError {
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// Under the title: the status and how long, then (`showsWhere`) where the thread works.
private struct ThreadStatusLine: View {
    let item: WorkItem
    let showsWhere: Bool

    var body: some View {
        HStack(spacing: 12) {
            HStack(spacing: 6) {
                WorkMark(status: item.status)
                Text(L10n.tr(item.status.word)).font(.footnote).foregroundStyle(Palette.secondary)
                WorkElapsed(item: item)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("thread-status")
            if showsWhere { WhereCopy(item: item) }
        }
    }
}

/// What a thread runs on, in its composer's row: the agent, then where it works unless that is its bot's own folder,
/// which goes without saying (the desktop's).
struct ThreadComposerChips: View {
    let bot: Bot
    let item: WorkItem
    @Environment(BotStore.self) private var model

    var body: some View {
        let agent = item.agent ?? bot.backend
        HStack(spacing: 8) {
            HStack(spacing: 6) {
                AgentIcon(registry: model.agentRegistry(agent), size: 14)
                Text(model.backendName(agent)).font(.footnote).lineLimit(1)
            }
            .foregroundStyle(Palette.secondary)
            .accessibilityElement(children: .combine)
            if !(bot.managedWorkspace && item.worktree == nil && item.folder == bot.cwd) {
                WhereCopy(item: item).layoutPriority(-1)
            }
        }
    }
}

/// Where a thread works; a tap copies the whole path.
private struct WhereCopy: View {
    let item: WorkItem
    @Environment(BotStore.self) private var model
    @State private var copied = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        if let place = item.whereLine(home: model.hello?.home ?? "") {
            if let path = place.path {
                Button {
                    Pasteboard.copy(path)
                    withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = true }
                } label: {
                    if copied {
                        Label(L10n.tr("Path copied"), systemImage: "checkmark").font(.footnote).foregroundStyle(Palette.tertiary)
                    } else {
                        WhereLabel(place: place)
                    }
                }
                .buttonStyle(PressScale())
                .accessibilityHint(L10n.tr("Copy path"))
                .accessibilityIdentifier("thread-where")
                // The words give the place back after a moment.
                .task(id: copied) {
                    guard copied, (try? await Task.sleep(for: .seconds(1.5))) != nil else { return }
                    withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { copied = false }
                }
            } else {
                WhereLabel(place: place)
            }
        }
    }
}

/// What the host told the thread, folded on top of it.
private struct ToldFold: View {
    let entry: Entry
    @Binding var isExpanded: Bool

    var body: some View {
        Disclosure(isExpanded: $isExpanded) {
            ToldText(entry: entry).transition(.opacity)
        } label: {
            Text(L10n.tr("What it was told")).font(.footnote.weight(.medium)).foregroundStyle(Palette.secondary)
        }
        .padding(.vertical, 6)
    }
}

/// The host's words to the thread: quiet, and not a bubble, since the user didn't type them.
private struct ToldText: View {
    let entry: Entry

    var body: some View {
        Text(entry.data.text ?? "")
            .font(.subheadline)
            .foregroundStyle(Palette.secondary)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

extension ChatItem {
    /// A thread's rows in the chat's form (the desktop's `buildChat`, Android `threadChatItems`): what it was told
    /// first, then its own entries with what it (`task`) put in the bot's `chat` where it came. Its steps are no
    /// rows (they show in the working line), nor, while it is `working`, the user's messages that wait above the composer.
    static func thread(_ transcript: ThreadEntries, chat: [Entry], task: String?, working: Bool) -> [ChatItem] {
        let own = transcript.hidingQueue(working: working)
        let shown = task.map { own.sharing(chat, of: $0) } ?? own
        return build((transcript.told.map { [$0] } ?? []) + shown)
    }

    /// What the host told the thread, which the user didn't type.
    var isToldToTask: Bool {
        if case let .entry(entry, _) = kind { entry.isToldToTask } else { false }
    }
}
