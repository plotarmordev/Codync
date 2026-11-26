import Foundation

/// An item's thread as a message is addressed to it: the bot and its item (what the host is told), and the task
/// whose transcript holds the thread's entries. The rules here read the same on Android and the desktop.
public struct WorkThread: Hashable, Sendable {
    public var botId: String
    public var itemId: String
    public var task: String

    public init(botId: String, itemId: String, task: String) {
        self.botId = botId
        self.itemId = itemId
        self.task = task
    }

    /// Where the composer keeps what is typed for it, apart from the bot's chat and gone with the bot.
    public var draftKey: String { "\(botId)/item/\(itemId)" }
}

/// Where an item's task works, in one quiet line: its branch when it has a worktree of its own, else its folder.
public struct WhereLine: Equatable, Sendable {
    public enum Kind: Sendable {
        case folder, branch

        public var symbol: String { self == .folder ? "folder" : "arrow.triangle.branch" }
    }

    public var kind: Kind
    public var text: String
    /// The whole path on the computer, which a tap copies; nil when the host didn't say.
    public var path: String?

    /// `path` with the computer's `home` folder as `~`, on either kind of separator; unchanged outside the home
    /// folder or without one.
    public static func homeRelative(_ path: String, home: String) -> String {
        let separators: Set<Character> = ["/", "\\"]
        var root = Substring(home)
        while let last = root.last, separators.contains(last) { root.removeLast() }
        guard !root.isEmpty, path.hasPrefix(root) else { return path }
        let rest = path.dropFirst(root.count)
        return rest.first.map(separators.contains) ?? true ? "~\(rest)" : path
    }
}

public extension WorkItem {
    /// Why its thread is only read (a phrase to look up): a routine's run and another bot's request have an owner
    /// for their result already. Nil for the bot's own item.
    var readOnlyLine: String? {
        switch source?.kind {
        case .routine: "Read only: a routine's run"
        case .request: "Read only: another bot's request"
        case nil: nil
        }
    }

    /// Nil before a task took it. A host that names no folder still shows a worktree's branch. `home`: the
    /// computer's home folder, which a folder shows as `~`.
    func whereLine(home: String) -> WhereLine? {
        let path = wherePath
        if let worktree { return WhereLine(kind: .branch, text: worktree.branch, path: path) }
        return path.map { WhereLine(kind: .folder, text: WhereLine.homeRelative($0, home: home), path: $0) }
    }

    /// Its thread, once a task has it.
    func thread(of botId: String) -> WorkThread? {
        task.map { WorkThread(botId: botId, itemId: id, task: $0) }
    }
}

public extension Bot {
    /// What its folder is to git, under the folder in Settings.
    enum GitState: Sendable {
        case repo, none, missing

        /// A phrase to look up.
        public var line: String {
            switch self {
            case .repo: "Git repository"
            case .none: "Not a git repository"
            case .missing: "Git isn't installed on this computer"
            }
        }
    }

    /// A host that doesn't say whether git is installed has it.
    var gitState: GitState {
        gitInstalled == false ? .missing : gitRepo ? .repo : .none
    }
}

public extension Entry {
    /// What a task was told (its item): the host wrote it, the user didn't type it.
    var isToldToTask: Bool { kind == "user" && data.from == "host" }

    /// The id of a message shown before the computer has it, by the nonce it was sent with.
    static func localId(_ nonce: String) -> String { "local-\(nonce)" }
}

/// A thread's transcript in two parts: what the host first told its task (folded on top), and everything else in
/// order. What the host told it later (an item picked up again) stays where it happened.
public struct ThreadEntries: Equatable, Sendable {
    public var told: Entry?
    public var rest: [Entry]

    /// While the thread works, the user's messages that wait above the composer (`queue(working:)`). What the host told
    /// it, the chat passing words on included, is never one of them: it stays quoted where it came.
    public func queue(working: Bool) -> [Entry] {
        rest.filter { !$0.isToldToTask }.queue(working: working)
    }

    /// `rest` without that queue.
    public func hidingQueue(working: Bool) -> [Entry] {
        working ? rest.filter { !$0.isQueued || $0.isToldToTask } : rest
    }
}

public extension Array where Element == Entry {
    /// A message the user took back is in neither part.
    var threadEntries: ThreadEntries {
        let shown = filter { $0.data.removed != true }
        let told = shown.first(where: \.isToldToTask)
        return ThreadEntries(told: told, rest: shown.filter { $0.id != told?.id })
    }

    /// A thread's entries with what its task `task` put in the bot's `chat` (a file, a voice note, an artifact, a
    /// card, answered or not) where it came (`createdAt`), each kept in its order; at the same
    /// moment the task's own first.
    func sharing(_ chat: [Entry], of task: String) -> [Entry] {
        var shared = chat.filter { $0.data.task?.id == task }[...]
        var out: [Entry] = []
        for e in self {
            while let next = shared.first, next.createdAt < e.createdAt { out.append(shared.removeFirst()) }
            out.append(e)
        }
        return out + shared
    }
}
