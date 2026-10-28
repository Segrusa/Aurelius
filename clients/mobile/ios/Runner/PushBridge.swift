import Flutter
import UIKit
import UserNotifications

/// Native side of `PushService` (lib/core/push/push_service.dart): permission,
/// the APNs device token, and taps on notifications. No Firebase: the PC
/// sends straight to APNs with the user's own key.
final class PushBridge: NSObject {
  static let shared = PushBridge()

  private var channel: FlutterMethodChannel?
  /// A tap that opened the app before Dart was ready.
  private var pendingTap: [String: Any]?

  /// The server sends `aps.category` "plan" or "approval"; both get the same
  /// two actions, which open the app (.foreground) so Dart can decide or show it.
  /// Registered at app start so the actions exist before the first push lands.
  static func registerCategories() {
    let approve = UNNotificationAction(identifier: "approve", title: "Goedkeuren", options: [.foreground])
    let view = UNNotificationAction(identifier: "view", title: "Bekijk", options: [.foreground])
    let categories = ["plan", "approval"].map {
      UNNotificationCategory(identifier: $0, actions: [approve, view], intentIdentifiers: [], options: [])
    }
    UNUserNotificationCenter.current().setNotificationCategories(Set(categories))
  }

  func attach(_ messenger: FlutterBinaryMessenger) {
    let channel = FlutterMethodChannel(name: "aurelius/push", binaryMessenger: messenger)
    channel.setMethodCallHandler { [weak self] call, result in
      self?.handle(call, result: result)
    }
    self.channel = channel
  }

  private func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "register":
      UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, _ in
        DispatchQueue.main.async {
          if granted {
            UIApplication.shared.registerForRemoteNotifications()
            result("granted")
          } else {
            result("denied")
          }
        }
      }
    case "initialTap":
      result(pendingTap)
      pendingTap = nil
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  func registered(_ deviceToken: Data) {
    let token = deviceToken.map { String(format: "%02x", $0) }.joined()
    channel?.invokeMethod("onToken", arguments: ["token": token, "environment": PushBridge.apnsEnvironment()])
  }

  func failed(_ error: Error) {
    channel?.invokeMethod("onError", arguments: error.localizedDescription)
  }

  /// [actionIdentifier] is the pressed action ("approve", "view"); a plain tap
  /// has none. It goes to Dart as `data["action"]`.
  func tapped(_ userInfo: [AnyHashable: Any], actionIdentifier: String = UNNotificationDefaultActionIdentifier) {
    var data = userInfo["aurelius"] as? [String: Any] ?? [:]
    if actionIdentifier == "approve" || actionIdentifier == "view" {
      data["action"] = actionIdentifier
    }
    guard let channel = channel else {
      pendingTap = data
      return
    }
    channel.invokeMethod("onTap", arguments: data) { [weak self] handled in
      if (handled as? Bool) != true {
        self?.pendingTap = data
      }
    }
  }

  /// Development-signed builds (Xcode) talk to the APNs sandbox; TestFlight
  /// and App Store builds to production. The provisioning profile says which.
  static func apnsEnvironment() -> String {
    guard
      let path = Bundle.main.path(forResource: "embedded", ofType: "mobileprovision"),
      let data = FileManager.default.contents(atPath: path),
      let text = String(data: data, encoding: .isoLatin1)
    else {
      return "production"
    }
    let pattern = "<key>aps-environment</key>\\s*<string>development</string>"
    return text.range(of: pattern, options: .regularExpression) != nil ? "sandbox" : "production"
  }
}
