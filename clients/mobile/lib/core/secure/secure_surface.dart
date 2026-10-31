import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// What a vault value may touch on this phone (E19): the clipboard as
/// sensitive content, and a screen that is kept out of screenshots, screen
/// recordings and the app switcher while a value shows. Nothing here logs or
/// keeps a value: a copy leaves only a token that tells whether the clipboard
/// still holds it.
abstract class SecureSurface {
  /// While true, Android sets `FLAG_SECURE` and iOS covers the app-switcher
  /// snapshot. Turning it off while the app is in the background waits until
  /// it is back, so the snapshot never shows the value.
  Future<void> setSecure(bool on);

  /// Put [text] on the clipboard as sensitive (Android 13+
  /// `EXTRA_IS_SENSITIVE`, iOS `localOnly` + an `expirationDate` [expiresIn]).
  /// The answer tells later whether the clipboard still holds it.
  Future<ClipToken?> copySensitive(String text, Duration expiresIn);

  /// Empty the clipboard if it still holds what [token] copied; leaves
  /// anything else (copied since) alone. True when it was cleared or is gone.
  /// False when it could not be checked now (Android reads the clipboard only
  /// in the foreground): try again later.
  Future<bool> clearIfOurs(ClipToken token);
}

/// What a copy left behind: a native marker, never the value.
class ClipToken {
  const ClipToken(this.id);

  final String id;

  @override
  String toString() => 'ClipToken';
}

/// The phone's own: the `aurelius/secure` channel (MainActivity.kt,
/// AppDelegate.swift). Elsewhere (the Linux test build) the plain clipboard
/// with a fingerprint of the text, and no screen protection.
class ChannelSecureSurface implements SecureSurface {
  ChannelSecureSurface({MethodChannel? channel, bool? native})
      : _channel = channel ?? const MethodChannel('aurelius/secure'),
        _native = native ?? (Platform.isAndroid || Platform.isIOS);

  final MethodChannel _channel;
  final bool _native;

  @override
  Future<void> setSecure(bool on) async {
    if (!_native) return;
    try {
      await _channel.invokeMethod<void>('setSecure', {'on': on});
    } on PlatformException {
      // Best effort: the value still hides after 30 s.
    } on MissingPluginException {
      // An older build without the channel.
    }
  }

  @override
  Future<ClipToken?> copySensitive(String text, Duration expiresIn) async {
    if (!_native) {
      await Clipboard.setData(ClipboardData(text: text));
      return ClipToken('fp:${text.hashCode}');
    }
    try {
      final id = await _channel.invokeMethod<String>('copySensitive', {'text': text, 'expiresInMs': expiresIn.inMilliseconds});
      return id == null ? null : ClipToken(id);
    } on PlatformException {
      return null;
    } on MissingPluginException {
      return null;
    }
  }

  @override
  Future<bool> clearIfOurs(ClipToken token) async {
    if (!_native) {
      final now = (await Clipboard.getData(Clipboard.kTextPlain))?.text;
      if (now != null && 'fp:${now.hashCode}' == token.id) await Clipboard.setData(const ClipboardData(text: ''));
      return true;
    }
    try {
      return await _channel.invokeMethod<bool>('clearIfOurs', {'id': token.id}) ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return true;
    }
  }
}

/// The one pending clipboard clear of the app (B6: 60 s after Copy). It
/// outlives the vault screen: leaving it does not keep the value on the
/// clipboard. A clear that cannot run in the background runs when the app is
/// back. A new copy replaces the pending one (the clipboard holds only that).
class SensitiveClipboard with WidgetsBindingObserver {
  SensitiveClipboard(this.surface);

  final SecureSurface surface;

  /// The app's one; a test sets its own.
  static SensitiveClipboard instance = SensitiveClipboard(ChannelSecureSurface());

  ClipToken? _pending;
  Timer? _timer;
  bool _observing = false;

  /// A clear waits.
  bool get pending => _pending != null;

  /// Copy [text] as sensitive and clear it after [after], if it is still ours then.
  Future<bool> copy(String text, Duration after) async {
    final token = await surface.copySensitive(text, after);
    if (token == null) return false;
    _pending = token;
    _timer?.cancel();
    _timer = Timer(after, _clear);
    if (!_observing) {
      _observing = true;
      WidgetsBinding.instance.addObserver(this);
    }
    return true;
  }

  Future<void> _clear() async {
    final token = _pending;
    if (token == null) return;
    // Not checked (in the background on Android): again once the app is back.
    if (!await surface.clearIfOurs(token)) return;
    if (identical(_pending, token)) _done();
  }

  void _done() {
    _pending = null;
    _timer?.cancel();
    _timer = null;
    if (_observing) {
      _observing = false;
      WidgetsBinding.instance.removeObserver(this);
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && _pending != null && !(_timer?.isActive ?? false)) unawaited(_clear());
  }

  /// Forget a pending clear (tests).
  void reset() => _done();
}
