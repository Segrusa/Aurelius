import CarPlay
import Flutter
import UIKit

/// CarPlay as a "voice-based conversational app" (iOS 26.4). It opens on the
/// list; the voice template comes up when Dart listens or speaks. The car gets its
/// own headless Flutter engine (`carPlayMain` in main.dart) with its own
/// connection, so the phone's scene and engine work the same with or
/// without a car. Dart decides what happens (lib/features/carplay); this side
/// shows the templates, listens and speaks, over the `aurelius/carplay` channel.
///
/// Dart → here: `lists` {sections}, `voice` {state}, `closeVoice`,
/// `listen` → text or nil, `speak` {text} → finished, `stop`, `popToRoot`.
/// Here → Dart: `select` id, `mic`, `stop`, `voiceClosed`, `interrupted`.
final class CarPlaySceneDelegate: UIResponder, CPTemplateApplicationSceneDelegate, CPInterfaceControllerDelegate {
  private var interfaceController: CPInterfaceController?
  private var engine: FlutterEngine?
  private var channel: FlutterMethodChannel?
  private let voice = CarPlayVoice()
  private var root: CPListTemplate?
  private var voiceTemplate: CPVoiceControlTemplate?
  private var voiceShown = false
  private var pendingState: String?

  func templateApplicationScene(_ templateApplicationScene: CPTemplateApplicationScene, didConnect interfaceController: CPInterfaceController) {
    self.interfaceController = interfaceController
    interfaceController.delegate = self
    let root = CPListTemplate(title: "Aurelius", sections: [])
    root.emptyViewTitleVariants = ["Connecting…"]
    self.root = root
    interfaceController.setRootTemplate(root, animated: false, completion: nil)
    voice.onInterrupted = { [weak self] in self?.channel?.invokeMethod("interrupted", arguments: nil) }
    startEngine()
  }

  func templateApplicationScene(_ templateApplicationScene: CPTemplateApplicationScene, didDisconnectInterfaceController interfaceController: CPInterfaceController) {
    voice.stop()
    channel?.setMethodCallHandler(nil)
    channel = nil
    engine?.destroyContext()
    engine = nil
    voiceTemplate = nil
    voiceShown = false
    root = nil
    self.interfaceController = nil
  }

  private func startEngine() {
    let engine = FlutterEngine(name: "carplay", project: nil, allowHeadlessExecution: true)
    engine.run(withEntrypoint: "carPlayMain")
    GeneratedPluginRegistrant.register(with: engine)
    let channel = FlutterMethodChannel(name: "aurelius/carplay", binaryMessenger: engine.binaryMessenger)
    channel.setMethodCallHandler { [weak self] call, result in
      self?.handle(call, result: result)
    }
    self.engine = engine
    self.channel = channel
  }

  private func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    let args = call.arguments as? [String: Any] ?? [:]
    switch call.method {
    case "lists":
      root?.updateSections((args["sections"] as? [[String: Any]] ?? []).map(section))
      result(nil)
    case "popToRoot":
      interfaceController?.popToRootTemplate(animated: true, completion: nil)
      result(nil)
    case "voice":
      showVoice(args["state"] as? String ?? "ready")
      result(nil)
    case "closeVoice":
      closeVoice()
      result(nil)
    case "listen":
      showVoice("listening")
      voice.listen { text, error in
        if let error {
          result(FlutterError(code: error, message: nil, details: nil))
        } else {
          result(text)
        }
      }
    case "speak":
      showVoice("speaking")
      voice.speak(args["text"] as? String ?? "") { finished in result(finished) }
    case "stop":
      voice.stop()
      result(nil)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  // MARK: Lists

  private func section(_ data: [String: Any]) -> CPListSection {
    let items = (data["items"] as? [[String: Any]] ?? []).map(item)
    return CPListSection(items: items, header: data["header"] as? String, sectionIndexTitle: nil)
  }

  /// An item with `children` opens a list of them; any other reports its id.
  private func item(_ data: [String: Any]) -> CPListItem {
    let text = data["text"] as? String ?? ""
    let item = CPListItem(text: text, detailText: data["detail"] as? String)
    let children = data["children"] as? [[String: Any]]
    if children != nil { item.accessoryType = .disclosureIndicator }
    item.handler = { [weak self] _, done in
      if let children {
        self?.push(title: text, items: children)
      } else if let id = data["id"] as? String {
        self?.channel?.invokeMethod("select", arguments: id)
      }
      done()
    }
    return item
  }

  private func push(title: String, items: [[String: Any]]) {
    let list = CPListTemplate(title: title, sections: [CPListSection(items: items.map(item))])
    interfaceController?.pushTemplate(list, animated: true, completion: nil)
  }

  // MARK: Voice

  /// Presents the voice template if needed and shows `state`: ready,
  /// listening, thinking, speaking or offline. Only status words, never the
  /// conversation itself.
  private func showVoice(_ state: String) {
    guard let interfaceController else { return }
    if let voiceTemplate {
      if voiceShown {
        voiceTemplate.activateVoiceControlState(withIdentifier: state)
      } else {
        pendingState = state
      }
      return
    }
    let template = makeVoiceTemplate()
    voiceTemplate = template
    voiceShown = false
    pendingState = state
    interfaceController.presentTemplate(template, animated: true) { [weak self] presented, _ in
      guard let self, self.voiceTemplate === template else { return }
      guard presented else {
        self.voiceTemplate = nil
        return
      }
      self.voiceShown = true
      if let state = self.pendingState { template.activateVoiceControlState(withIdentifier: state) }
      self.pendingState = nil
    }
  }

  private func closeVoice() {
    guard voiceTemplate != nil else { return }
    voice.stop()
    interfaceController?.dismissTemplate(animated: true, completion: nil)
  }

  private func makeVoiceTemplate() -> CPVoiceControlTemplate {
    func state(_ id: String, _ titles: [String], _ symbol: String) -> CPVoiceControlState {
      CPVoiceControlState(identifier: id, titleVariants: titles, image: UIImage(systemName: symbol), repeats: false)
    }
    let states = [
      state("ready", ["Tap the microphone", "Ready"], "mic.circle"),
      state("listening", ["Listening…"], "waveform"),
      state("thinking", ["Working on it…", "Working…"], "ellipsis.bubble"),
      state("speaking", ["Speaking…"], "speaker.wave.2"),
      state("offline", ["No connection"], "wifi.slash"),
    ]
    let template = CPVoiceControlTemplate(voiceControlStates: states)
    #if compiler(>=6.3)
      if #available(iOS 26.4, *) {
        let mic = CPButton(image: UIImage(systemName: "mic.fill") ?? UIImage()) { [weak self] _ in
          self?.channel?.invokeMethod("mic", arguments: nil)
        }
        mic.title = "Talk"
        let stop = CPButton(image: UIImage(systemName: "stop.fill") ?? UIImage()) { [weak self] _ in
          self?.voice.stop()
          self?.channel?.invokeMethod("stop", arguments: nil)
        }
        stop.title = "Stop"
        for state in states {
          state.actionButtons = state.identifier == "ready" || state.identifier == "offline" ? [mic] : [stop]
        }
        template.leadingNavigationBarButtons = [
          CPBarButton(title: "Lists") { [weak self] _ in self?.closeVoice() }
        ]
      }
    #endif
    return template
  }

  func templateDidDisappear(_ aTemplate: CPTemplate, animated: Bool) {
    guard let voiceTemplate, aTemplate === voiceTemplate else { return }
    self.voiceTemplate = nil
    voiceShown = false
    voice.stop()
    channel?.invokeMethod("voiceClosed", arguments: nil)
  }
}
