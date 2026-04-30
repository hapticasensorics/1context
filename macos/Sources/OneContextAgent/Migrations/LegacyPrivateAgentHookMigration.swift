import Foundation

enum LegacyPrivateAgentHookMigration {
  static func isStartupHookCommand(_ command: String) -> Bool {
    command.contains("/1Context-private-4/tools/wiki-startup-context.py")
  }

  static func isStatusLineCommand(_ command: String) -> Bool {
    command.contains("/1Context-private-4/tools/wiki-statusline.py")
  }

  static func isHookCommand(_ command: String) -> Bool {
    isStartupHookCommand(command) || isStatusLineCommand(command)
  }
}
