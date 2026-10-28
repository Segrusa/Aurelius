# aurelius_mobile

Aurelius companion for iPhone and Android

## Getting Started

This project is a starting point for a Flutter application.

A few resources to get you started if this is your first Flutter project:

- [Learn Flutter](https://docs.flutter.dev/get-started/learn-flutter)
- [Write your first Flutter app](https://docs.flutter.dev/get-started/codelab)
- [Flutter learning resources](https://docs.flutter.dev/reference/learning-resources)

For help getting started with Flutter development, view the
[online documentation](https://docs.flutter.dev/), which offers tutorials,
samples, guidance on mobile development, and a full API reference.

## CarPlay

On iPhone the app is also a CarPlay "voice-based conversational app"
(iOS 26.4 and later, CarPlay Developer Guide of February 2026). It opens on
its list; tapping an item opens the voice screen. What you say is recognized
on the iPhone (when the language allows it). Talking sends it to the project
assistant of the chosen manager's project (managers work through tasks only);
the answer is read aloud without markdown
or code, and a long answer only up to its first part (the rest stays in the
conversation on the phone). There is no wake word: tap the microphone (or an
item in the list) to say something.

The list (button "Lists" in the voice screen) shows:

- **Talk to the … assistant**: the project of the chosen manager, or the
  connection status when the PC is not reachable.
- **Log a task** / **Log an idea**: say it, and it is logged in the chosen
  manager's project. The first sentence becomes the title, everything you said
  the goal or note. A task is a draft without a manager, so nothing starts
  until you assign it on the phone or PC.
- **Needs you**: "Hear what needs you" reads the waiting tasks out; a task reads
  its latest message and takes your answer by voice into its conversation.
  Plans and approvals are decided on the phone (Inbox).
- **Recent managers** and **Projects** → managers: pick who you talk to (most
  recently active first). The choice is kept per paired PC.

CarPlay runs its own Flutter engine (`carPlayMain` in `lib/main.dart`, logic in
`lib/features/carplay`) with its own connection to the active PC, so the phone
works the same with or without a car. Native side:
`ios/Runner/CarPlaySceneDelegate.swift` (templates) and `CarPlayVoice.swift`
(speech recognition, speech, audio session).

### Testing in the CarPlay Simulator

1. Open `ios/Runner.xcworkspace` in Xcode 26.4 or later and run the app on an
   iOS 26.4+ simulator. Pair it with a PC as usual.
2. In the Simulator: **I/O → External Displays → CarPlay**. Aurelius appears on
   the CarPlay screen; open it.
3. It opens on the list. Tap "Talk to …" or "Log a task"; the simulator uses
   the Mac's microphone; allow microphone and speech recognition when the
   iPhone asks.
4. Also try: "Log an idea", picking another manager, "Needs you", turning the PC's
   server off (CarPlay says it has no connection and recovers by itself) and a
   call or Siri during an answer (speech stops, tap the microphone again).

Simulator builds sign with `ios/Runner/CarPlay.entitlements`, which adds
`com.apple.developer.carplay-voice-based-conversation`. Device builds still
sign with `Runner.entitlements` without it, because a provisioning profile
can only hold the key after Apple approved it.

**TestFlight and App Store builds need the CarPlay entitlement approved by
Apple** (request it at developer.apple.com/carplay for the voice-based
conversational category). Once approved, enable it for the App ID, add the
key to `Runner.entitlements` and remove the
`CODE_SIGN_ENTITLEMENTS[sdk=iphonesimulator*]` lines from the Xcode project.
