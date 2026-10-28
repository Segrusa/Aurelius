import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../app/router.dart';
import '../../data/aurelius_repository.dart';

enum PushState { unsupported, off, denied, registered, failed }

/// Push notifications through APNs, sent by the PC itself (no hosted
/// backend). The native side (ios/Runner/PushBridge.swift) asks for
/// permission, hands over the device token and reports taps.
class PushService {
  PushService({MethodChannel? channel}) : _channel = channel ?? const MethodChannel('aurelius/push') {
    _channel.setMethodCallHandler(_onCall);
  }

  final MethodChannel _channel;
  final ValueNotifier<PushState> state = ValueNotifier(Platform.isIOS ? PushState.off : PushState.unsupported);
  String? lastError;

  AureliusRepository? _repository;
  AppRouter? _router;
  StreamSubscription<void>? _online;
  String? _token;
  String? _environment;

  static const _enabledKey = 'push.enabled';

  bool get supported => Platform.isIOS;

  /// A host app is up: register again (tokens can change), route a tap that
  /// opened the app.
  Future<void> attach(AureliusRepository repository, AppRouter router) async {
    _repository = repository;
    _router = router;
    if (!supported) return;
    final prefs = await SharedPreferences.getInstance();
    if (prefs.getBool(_enabledKey) ?? false) {
      unawaited(_requestToken());
      // The session may not exist yet; send the token once online.
      _online = repository.online.listen((_) => _sendToken());
    }
    try {
      final tap = await _channel.invokeMapMethod<String, dynamic>('initialTap');
      if (tap != null) router.openNotification(tap, repository);
    } on MissingPluginException {
      return;
    }
  }

  void detach(AureliusRepository repository) {
    if (_repository != repository) return;
    _online?.cancel();
    _online = null;
    _repository = null;
    _router = null;
  }

  Future<void> enable(AureliusRepository repository) async {
    (await SharedPreferences.getInstance()).setBool(_enabledKey, true);
    _online ??= repository.online.listen((_) => _sendToken());
    await _requestToken();
  }

  Future<void> disable(AureliusRepository repository) async {
    (await SharedPreferences.getInstance()).setBool(_enabledKey, false);
    await _online?.cancel();
    _online = null;
    try {
      await repository.unregisterPush();
    } catch (_) {}
    state.value = PushState.off;
  }

  Future<void> _requestToken() async {
    try {
      final answer = await _channel.invokeMethod<String>('register');
      if (answer == 'denied') state.value = PushState.denied;
    } on MissingPluginException {
      state.value = PushState.unsupported;
    } on PlatformException catch (e) {
      lastError = e.message;
      state.value = PushState.failed;
    }
  }

  Future<void> _sendToken() async {
    final repository = _repository;
    final token = _token;
    if (repository == null || token == null || !repository.linkState.isOnline) return;
    try {
      await repository.registerPush(token: token, environment: _environment ?? 'production');
      state.value = PushState.registered;
    } catch (e) {
      lastError = e.toString();
      state.value = PushState.failed;
    }
  }

  Future<dynamic> _onCall(MethodCall call) async {
    switch (call.method) {
      case 'onToken':
        final args = Map<String, dynamic>.from(call.arguments as Map);
        _token = args['token'] as String?;
        _environment = args['environment'] as String?;
        await _sendToken();
        return null;
      case 'onError':
        lastError = call.arguments as String?;
        state.value = PushState.failed;
        return null;
      case 'onTap':
        final router = _router;
        final repository = _repository;
        if (router == null || repository == null) return false;
        router.openNotification(Map<String, dynamic>.from(call.arguments as Map), repository);
        return true;
    }
    return null;
  }
}
