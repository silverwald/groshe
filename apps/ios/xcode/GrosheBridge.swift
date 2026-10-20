import Foundation

public final class GrosheBridge {
    public static let shared = GrosheBridge()

    private init() {}

    public func initialize() -> Bool {
        guard let supportDir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            print("Failed to locate application support directory")
            return false
        }

        let grosheDir = supportDir.appendingPathComponent("Groshe", isDirectory: true)
        try? FileManager.default.createDirectory(at: grosheDir, withIntermediateDirectories: true)

        let pathCStr = grosheDir.path.cString(using: .utf8)
        let status = groshe_ios_init(pathCStr)
        if status != 0 {
            print("groshe_ios_init failed with status \(status)")
            return false
        }
        return true
    }

    public func run() {
        _ = groshe_ios_run()
    }

    public func onForeground() {
        _ = groshe_ios_on_foreground()
    }

    public func onBackground() {
        _ = groshe_ios_on_background()
    }

    public func cleanup() {
        _ = groshe_ios_cleanup()
    }
}
