import AVFoundation
import Speech

/// Listening (speech recognition, on the device when it can) and speaking
/// for CarPlay. The audio session is active only while one of them runs, and
/// ducks other audio (music, navigation) meanwhile.
final class CarPlayVoice: NSObject, AVSpeechSynthesizerDelegate {
  /// A phone call, Siri or another app took the audio: what ran was stopped.
  var onInterrupted: (() -> Void)?

  private let synthesizer = AVSpeechSynthesizer()
  private let audioEngine = AVAudioEngine()
  private var recognizer: SFSpeechRecognizer?
  private var request: SFSpeechAudioBufferRecognitionRequest?
  private var task: SFSpeechRecognitionTask?
  private var heard: ((String?, String?) -> Void)?
  private var spoken: ((Bool) -> Void)?
  private var speaking: AVSpeechUtterance?
  private var silence: Timer?
  private var transcript = ""

  /// Seconds of quiet that end what the user says, and the wait for a first word.
  private let pause: TimeInterval = 1.6
  private let firstWord: TimeInterval = 8

  override init() {
    super.init()
    synthesizer.delegate = self
    NotificationCenter.default.addObserver(
      self, selector: #selector(interrupted(_:)), name: AVAudioSession.interruptionNotification, object: nil)
  }

  deinit {
    NotificationCenter.default.removeObserver(self)
  }

  var isBusy: Bool { heard != nil || spoken != nil }

  // MARK: Listening

  /// Calls `done` once with what was said (nil: nothing) or an error code:
  /// `denied` (microphone or speech recognition not allowed), `unavailable`
  /// or `audio`.
  func listen(done: @escaping (String?, String?) -> Void) {
    stop()
    authorize { [weak self] allowed in
      guard let self else { return }
      guard allowed else { return done(nil, "denied") }
      guard let recognizer = SFSpeechRecognizer(locale: Locale.current) ?? SFSpeechRecognizer(), recognizer.isAvailable else {
        return done(nil, "unavailable")
      }
      let request = SFSpeechAudioBufferRecognitionRequest()
      request.shouldReportPartialResults = true
      if recognizer.supportsOnDeviceRecognition { request.requiresOnDeviceRecognition = true }
      if #available(iOS 16, *) { request.addsPunctuation = true }
      do {
        try self.activate(record: true)
        let input = self.audioEngine.inputNode
        input.removeTap(onBus: 0)
        input.installTap(onBus: 0, bufferSize: 1024, format: input.outputFormat(forBus: 0)) { buffer, _ in
          request.append(buffer)
        }
        self.audioEngine.prepare()
        try self.audioEngine.start()
      } catch {
        self.audioEngine.inputNode.removeTap(onBus: 0)
        self.deactivate()
        return done(nil, "audio")
      }
      self.recognizer = recognizer
      self.request = request
      self.transcript = ""
      self.heard = done
      self.task = recognizer.recognitionTask(with: request) { [weak self] result, error in
        DispatchQueue.main.async {
          guard let self, self.request === request else { return }
          if let result {
            self.transcript = result.bestTranscription.formattedString
            if result.isFinal { return self.finishListening() }
            self.wait(self.pause)
          } else if error != nil {
            self.finishListening()
          }
        }
      }
      self.wait(self.firstWord)
    }
  }

  private func wait(_ seconds: TimeInterval) {
    silence?.invalidate()
    silence = Timer.scheduledTimer(withTimeInterval: seconds, repeats: false) { [weak self] _ in
      self?.finishListening()
    }
  }

  private func finishListening(cancelled: Bool = false) {
    silence?.invalidate()
    silence = nil
    if audioEngine.isRunning { audioEngine.stop() }
    audioEngine.inputNode.removeTap(onBus: 0)
    request?.endAudio()
    task?.cancel()
    request = nil
    task = nil
    let done = heard
    heard = nil
    let text = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
    deactivate()
    done?(cancelled || text.isEmpty ? nil : text, nil)
  }

  private func authorize(_ done: @escaping (Bool) -> Void) {
    SFSpeechRecognizer.requestAuthorization { status in
      guard status == .authorized else { return DispatchQueue.main.async { done(false) } }
      let granted: (Bool) -> Void = { ok in DispatchQueue.main.async { done(ok) } }
      if #available(iOS 17, *) {
        AVAudioApplication.requestRecordPermission(completionHandler: granted)
      } else {
        AVAudioSession.sharedInstance().requestRecordPermission(granted)
      }
    }
  }

  // MARK: Speaking

  /// Calls `done(true)` when all was said, `done(false)` when it was stopped.
  func speak(_ text: String, done: @escaping (Bool) -> Void) {
    stop()
    do {
      try activate(record: false)
    } catch {
      return done(false)
    }
    let utterance = AVSpeechUtterance(string: text)
    utterance.voice = AVSpeechSynthesisVoice(language: AVSpeechSynthesisVoice.currentLanguageCode())
    if #available(iOS 14, *) { utterance.prefersAssistiveTechnologySettings = true }
    spoken = done
    speaking = utterance
    synthesizer.speak(utterance)
  }

  // A stopped utterance reports its cancel later; only the current one counts.
  func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
    if utterance === speaking { finishSpeaking(true) }
  }

  func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
    if utterance === speaking { finishSpeaking(false) }
  }

  private func finishSpeaking(_ completed: Bool) {
    let done = spoken
    spoken = nil
    speaking = nil
    deactivate()
    done?(completed)
  }

  // MARK: Session

  /// Stops listening and speaking; their callbacks get "nothing" / false.
  func stop() {
    if heard != nil { finishListening(cancelled: true) }
    if spoken != nil {
      let done = spoken
      spoken = nil
      speaking = nil
      synthesizer.stopSpeaking(at: .immediate)
      deactivate()
      done?(false)
    }
  }

  private func activate(record: Bool) throws {
    let session = AVAudioSession.sharedInstance()
    if record {
      try session.setCategory(.playAndRecord, mode: .default, options: [.duckOthers, .allowBluetoothA2DP, .defaultToSpeaker])
    } else {
      // Spoken answers pause podcasts and duck music, like navigation prompts.
      try session.setCategory(.playback, mode: .voicePrompt, options: [.duckOthers, .interruptSpokenAudioAndMixWithOthers])
    }
    try session.setActive(true)
  }

  private func deactivate() {
    guard heard == nil, spoken == nil else { return }
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
  }

  @objc private func interrupted(_ notification: Notification) {
    guard
      let raw = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
      AVAudioSession.InterruptionType(rawValue: raw) == .began,
      isBusy
    else { return }
    DispatchQueue.main.async { [weak self] in
      self?.stop()
      self?.onInterrupted?()
    }
  }
}
