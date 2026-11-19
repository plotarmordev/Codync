import CodyncKit
import Foundation
import Testing

/// Where the running app keeps its pairings and settings: the App Group named in its Info.plist, so a copy built with
/// its own group (`apps/ios/own-build.sh`) never shares the environment's group with the App Store app.
struct AppGroupTests {
    private static let group = Bundle.main.object(forInfoDictionaryKey: "CodyncAppGroup") as? String
    private static let environmentGroup = AppEnvironment.current.appGroup

    @Test(.enabled(if: group != environmentGroup, "needs a build with its own CODYNC_APP_GROUP"))
    func keepsPairingsAndSettingsInTheBuildsOwnGroup() throws {
        let group = try #require(Self.group)
        let computer = Computer(id: "app-group-test", name: "App Group test", signKey: "key")
        let context = SharedStore.Context(accountID: nil)
        let (computers, lead) = (context.computers, context.shortcutLeadComputer)
        defer { (context.computers, context.shortcutLeadComputer) = (computers, lead) }

        context.computers = [computer]
        context.shortcutLeadComputer = computer.id

        let own = SharedStore.Context(accountID: nil, suite: group)
        #expect(own.computers == [computer])
        #expect(own.shortcutLeadComputer == computer.id)
        let environment = SharedStore.Context(accountID: nil, suite: Self.environmentGroup)
        #expect(!environment.computers.contains(computer))
        #expect(environment.shortcutLeadComputer != computer.id)
    }
}
