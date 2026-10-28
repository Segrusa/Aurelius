import 'dart:async';

import 'package:flutter/services.dart';

/// What the voice template shows: status words only, never the conversation.
enum VoiceState { ready, listening, thinking, speaking, offline }

/// One row of a CarPlay list. With [children] it opens a list of them;
/// otherwise a tap reports [id].
class CarItem {
  const CarItem({this.id, required this.text, this.detail, this.children});

  final String? id;
  final String text;
  final String? detail;
  final List<CarItem>? children;

  Map<String, dynamic> toJson() => {
        'id': ?id,
        'text': text,
        'detail': ?detail,
        if (children != null) 'children': [for (final c in children!) c.toJson()],
      };
}

class CarSection {
  const CarSection({this.header, required this.items});

  final String? header;
  final List<CarItem> items;

  Map<String, dynamic> toJson() => {'header': ?header, 'items': [for (final i in items) i.toJson()]};
}

/// What the car reports: a list row ([id]), the microphone or stop button,
/// the voice screen closed, or the audio taken by a call or Siri.
enum CarEventKind { select, mic, stop, voiceClosed, interrupted }

class CarEvent {
  const CarEvent(this.kind, [this.id]);

  final CarEventKind kind;
  final String? id;
}

/// Listening failed: `denied` (no microphone or speech permission),
/// `unavailable` (no recognizer for the language) or `audio`.
class CarVoiceException implements Exception {
  const CarVoiceException(this.code);

  final String code;
}

/// The CarPlay templates, microphone and speaker (CarPlaySceneDelegate.swift).
abstract class CarScreen {
  Stream<CarEvent> get events;
  Future<void> lists(List<CarSection> sections);
  Future<void> popToRoot();
  Future<void> voice(VoiceState state);
  Future<void> closeVoice();

  /// What the user said; null when nothing was heard or it was stopped.
  Future<String?> listen();

  /// True when all was said, false when it was stopped.
  Future<bool> speak(String text);
  Future<void> stop();
}

class ChannelCarScreen implements CarScreen {
  ChannelCarScreen([MethodChannel? channel]) : _channel = channel ?? const MethodChannel('aurelius/carplay') {
    _channel.setMethodCallHandler((call) async {
      final kind = switch (call.method) {
        'select' => CarEventKind.select,
        'mic' => CarEventKind.mic,
        'stop' => CarEventKind.stop,
        'voiceClosed' => CarEventKind.voiceClosed,
        'interrupted' => CarEventKind.interrupted,
        _ => null,
      };
      if (kind != null) _events.add(CarEvent(kind, call.arguments as String?));
    });
  }

  final MethodChannel _channel;
  final _events = StreamController<CarEvent>.broadcast();

  @override
  Stream<CarEvent> get events => _events.stream;

  @override
  Future<void> lists(List<CarSection> sections) => _channel.invokeMethod('lists', {'sections': [for (final s in sections) s.toJson()]});

  @override
  Future<void> popToRoot() => _channel.invokeMethod('popToRoot');

  @override
  Future<void> voice(VoiceState state) => _channel.invokeMethod('voice', {'state': state.name});

  @override
  Future<void> closeVoice() => _channel.invokeMethod('closeVoice');

  @override
  Future<String?> listen() async {
    try {
      return await _channel.invokeMethod<String>('listen');
    } on PlatformException catch (e) {
      throw CarVoiceException(e.code);
    }
  }

  @override
  Future<bool> speak(String text) async => await _channel.invokeMethod<bool>('speak', {'text': text}) ?? false;

  @override
  Future<void> stop() => _channel.invokeMethod('stop');
}
