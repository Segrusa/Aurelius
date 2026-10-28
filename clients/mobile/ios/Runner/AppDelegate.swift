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
