import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'screen_counter.dart';

/// The one SharedPreferences key that holds the counts (JSON), on this device only.
const screenUseKey = 'screen_use.v1';

/// Writes are held back this long, so a burst of navigation is one write.
const screenUseWriteDelayMs = 2000;

/// The device's screen counter: loads and saves a [ScreenCounter] in
/// SharedPreferences (debounced) and is the router's observer, the one place
/// that counts a screen. Never touches the network or the server.
class ScreenUsage extends NavigatorObserver with ChangeNotifier {
  ScreenUsage({int Function()? clock})
    : _clock = clock ?? (() => DateTime.now().millisecondsSinceEpoch) {
    _loaded = _load();
  }

  final int Function() _clock;
  final ScreenCounter counter = ScreenCounter();
  late final Future<void> _loaded;
  Timer? _timer;
  bool _dirty = false;
  bool _disposed = false;
  String? _tab;

  /// Completes once what was stored is merged in.
  Future<void> get loaded => _loaded;

  Future<void> _load() async {
    try {
      final raw = (await SharedPreferences.getInstance()).getString(
        screenUseKey,
      );
      counter.merge(ScreenCounter.fromJsonString(raw));
      if (!_disposed) notifyListeners();
    } catch (_) {
      // Counting is best effort: without storage it only lives in memory.
    }
  }

  /// A tab of the shell became the selected one.
  void openTab(String pattern) {
    _tab = pattern;
    _count(pattern);
  }

  void _count(String pattern) {
    if (!counter.open(pattern, _clock())) return;
    _changed();
  }

  void _changed() {
    _dirty = true;
    _timer?.cancel();
    _timer = Timer(const Duration(milliseconds: screenUseWriteDelayMs), flush);
    if (!_disposed) notifyListeners();
  }

  /// Write now what is not written yet.
  Future<void> flush() async {
    _timer?.cancel();
    if (!_dirty) return;
    _dirty = false;
    try {
      await _loaded;
      await (await SharedPreferences.getInstance()).setString(
        screenUseKey,
        counter.toJsonString(),
      );
    } catch (_) {}
  }

  /// "Reset counts": forget everything, here and in storage.
  Future<void> reset() async {
    counter.reset();
    _dirty = true;
    notifyListeners();
    await flush();
  }

  /// The route pattern of a page route (its page name); null for dialogs,
  /// sheets and the shell itself.
  static String? _pattern(Route<dynamic>? route) {
    final name = route?.settings.name;
    return route is PageRoute && name != null && name.startsWith('/')
        ? name
        : null;
  }

  @override
  void didPush(Route<dynamic> route, Route<dynamic>? previousRoute) {
    final pattern = _pattern(route);
    if (pattern != null) _count(pattern);
  }

  @override
  void didPop(Route<dynamic> route, Route<dynamic>? previousRoute) {
    if (_pattern(route) == null) return;
    // Back to what was there: a tab when the page below is the shell.
    final below = _pattern(previousRoute) ?? _tab;
    if (below != null) counter.resume(below);
  }

  @override
  void dispose() {
    _disposed = true;
    unawaited(flush());
    super.dispose();
  }
}
