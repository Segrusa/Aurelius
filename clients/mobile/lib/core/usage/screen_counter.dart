import 'dart:convert';

import 'package:equatable/equatable.dart';

/// How often one screen was opened on this device, and when first and last.
class ScreenStat extends Equatable {
  const ScreenStat({
    required this.opens,
    required this.firstMs,
    required this.lastMs,
  });

  final int opens;
  final int firstMs;
  final int lastMs;

  Map<String, dynamic> toJson() => {
    'opens': opens,
    'first_ms': firstMs,
    'last_ms': lastMs,
  };

  factory ScreenStat.fromJson(Map<String, dynamic> json) => ScreenStat(
    opens: json['opens'] as int,
    firstMs: json['first_ms'] as int,
    lastMs: json['last_ms'] as int,
  );

  @override
  List<Object?> get props => [opens, firstMs, lastMs];
}

/// Counts screen openings by route pattern ("/ticket/:tid", never the real id).
/// Pure: no clock, no storage, no network; the caller passes the time and
/// keeps the JSON.
class ScreenCounter {
  ScreenCounter([Map<String, ScreenStat> stats = const {}])
    : _stats = {...stats};

  final Map<String, ScreenStat> _stats;
  String? _current;

  /// Every screen seen so far, by route pattern.
  Map<String, ScreenStat> get stats => Map.unmodifiable(_stats);

  /// The screen on top now, if any.
  String? get current => _current;

  /// A screen opened (push or tab switch). The same screen right after itself
  /// is not a new opening. True when it was counted.
  bool open(String pattern, int nowMs) {
    if (pattern == _current) return false;
    _current = pattern;
    final old = _stats[pattern];
    _stats[pattern] = ScreenStat(
      opens: (old?.opens ?? 0) + 1,
      firstMs: old?.firstMs ?? nowMs,
      lastMs: nowMs,
    );
    return true;
  }

  /// A screen is on top again because the one above it closed: not an opening.
  void resume(String pattern) => _current = pattern;

  /// Screens by opens, most used first; the most recent first among equals.
  List<MapEntry<String, ScreenStat>> ranked() => _stats.entries.toList()
    ..sort((a, b) {
      final byOpens = b.value.opens.compareTo(a.value.opens);
      return byOpens != 0 ? byOpens : b.value.lastMs.compareTo(a.value.lastMs);
    });

  /// Add what another counter holds (what was stored before this one loaded).
  void merge(ScreenCounter other) {
    for (final e in other._stats.entries) {
      final mine = _stats[e.key];
      _stats[e.key] = mine == null
          ? e.value
          : ScreenStat(
              opens: mine.opens + e.value.opens,
              firstMs: mine.firstMs < e.value.firstMs
                  ? mine.firstMs
                  : e.value.firstMs,
              lastMs: mine.lastMs > e.value.lastMs
                  ? mine.lastMs
                  : e.value.lastMs,
            );
    }
  }

  /// Forget every count.
  void reset() {
    _stats.clear();
    _current = null;
  }

  String toJsonString() =>
      jsonEncode({for (final e in _stats.entries) e.key: e.value.toJson()});

  /// Reads what [toJsonString] wrote; anything unreadable gives an empty counter.
  factory ScreenCounter.fromJsonString(String? raw) {
    if (raw == null) return ScreenCounter();
    try {
      final map = jsonDecode(raw) as Map<String, dynamic>;
      return ScreenCounter({
        for (final e in map.entries)
          e.key: ScreenStat.fromJson(e.value as Map<String, dynamic>),
      });
    } catch (_) {
      return ScreenCounter();
    }
  }
}
