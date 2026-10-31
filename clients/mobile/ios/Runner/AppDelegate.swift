import Flutter
import UIKit
import UserNotifications

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    UNUserNotificationCenter.current().delegate = self
    PushBridge.registerCategories()
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    if let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "AureliusPush") {
      PushBridge.shared.attach(registrar.messenger())
    }
    if let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "AureliusClipboard") {
      attachClipboard(registrar.messenger())
    }
    if let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "AureliusSecure") {
      SecureScreen.shared.attach(registrar.messenger())
    }
  }

  /// Images on the clipboard: Flutter's text fields only paste text.
  /// `hasImage` does not read the pasteboard, so it shows no paste prompt.
  private func attachClipboard(_ messenger: FlutterBinaryMessenger) {
    let channel = FlutterMethodChannel(name: "aurelius/clipboard", binaryMessenger: messenger)
    channel.setMethodCallHandler { call, result in
      switch call.method {
      case "hasImage":
        result(UIPasteboard.general.hasImages)
      case "image":
        guard let image = UIPasteboard.general.image else {
          result(nil)
          return
        }
        if let png = image.pngData(), png.count <= 8 * 1024 * 1024 {
          result(["bytes": FlutterStandardTypedData(bytes: png), "extension": "png"])
        } else if let jpeg = image.jpegData(compressionQuality: 0.85) {
          result(["bytes": FlutterStandardTypedData(bytes: jpeg), "extension": "jpg"])
        } else {
          result(nil)
        }
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  override func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
    PushBridge.shared.registered(deviceToken)
    super.application(application, didRegisterForRemoteNotificationsWithDeviceToken: deviceToken)
  }

  override func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
    PushBridge.shared.failed(error)
    super.application(application, didFailToRegisterForRemoteNotificationsWithError: error)
  }

  override func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    PushBridge.shared.tapped(response.notification.request.content.userInfo, actionIdentifier: response.actionIdentifier)
    completionHandler()
  }

  override func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    completionHandler([.banner, .list, .sound])
  }
}

/// Vault values (E19): the clipboard as local-only content that expires, and
/// a cover over the app-switcher snapshot while a value shows. Never logs a
/// value; a copy is remembered by the pasteboard's change count only.
final class SecureScreen {
  static let shared = SecureScreen()

  /// A value shows in the app (`setSecure`).
  private var wanted = false
  private var covers: [UIView] = []
  private var clipId: String?
  private var clipChangeCount = 0

  func attach(_ messenger: FlutterBinaryMessenger) {
    let center = NotificationCenter.default
    center.addObserver(forName: UIScene.willDeactivateNotification, object: nil, queue: .main) { [weak self] note in
      self?.cover(note.object as? UIWindowScene)
    }
    // The phone's window scene only (CarPlay has its own scene).
    center.addObserver(forName: UIScene.didActivateNotification, object: nil, queue: .main) { [weak self] note in
      if note.object is UIWindowScene { self?.uncover() }
    }
    let channel = FlutterMethodChannel(name: "aurelius/secure", binaryMessenger: messenger)
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(nil)
        return
      }
      let args = call.arguments as? [String: Any]
      switch call.method {
      case "setSecure":
        // Off takes effect only when the app is active again (didActivate):
        // the snapshot is taken on the way out.
        self.wanted = args?["on"] as? Bool ?? false
        result(nil)
      case "copySensitive":
        guard let text = args?["text"] as? String else {
          result(nil)
          return
        }
        let ms = args?["expiresInMs"] as? Int ?? 60_000
        UIPasteboard.general.setItems(
          [["public.utf8-plain-text": text]],
          options: [.localOnly: true, .expirationDate: Date().addingTimeInterval(Double(ms) / 1000)]
        )
        let id = UUID().uuidString
        self.clipId = id
        self.clipChangeCount = UIPasteboard.general.changeCount
        result(id)
      case "clearIfOurs":
        // Compared by change count: reading the pasteboard would ask the user.
        if let id = args?["id"] as? String, id == self.clipId, UIPasteboard.general.changeCount == self.clipChangeCount {
          UIPasteboard.general.items = []
        }
        self.clipId = nil
        result(true)
      default:
        result(FlutterMethodNotImplemented)
      }
    }
  }

  private func cover(_ scene: UIWindowScene?) {
    guard wanted, covers.isEmpty, let scene else { return }
    for window in scene.windows {
      let blur = UIVisualEffectView(effect: UIBlurEffect(style: .systemThickMaterial))
      blur.frame = window.bounds
      blur.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      window.addSubview(blur)
      covers.append(blur)
    }
  }

  private func uncover() {
    for view in covers {
      view.removeFromSuperview()
    }
    covers = []
  }
}
