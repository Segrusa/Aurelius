import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../data/everyone.dart';
import '../../data/protocol/models.dart';

/// Under the composer: whom the `@Everyone` in the text reaches, before it is
/// sent ("Wakes 5 employees (1 paused) and notifies 2 board members").
///
/// It asks [load] once per thread ([channel]: the channel's or discussion's
/// conversation, null General) after the text starts to contain `@Everyone`,
/// not per keystroke. A failure or an older server shows nothing; sending is
/// never held up by it.
class EveryoneAudienceLine extends StatefulWidget {
  const EveryoneAudienceLine({super.key, required this.text, required this.channel, required this.load});

  final ValueListenable<TextEditingValue> text;
  final String? channel;
  final Future<DiscussionAudience> Function(String? channel) load;

  @override
  State<EveryoneAudienceLine> createState() => _EveryoneAudienceLineState();
}

class _EveryoneAudienceLineState extends State<EveryoneAudienceLine> {
  Timer? _timer;
  DiscussionAudience? _audience;
  bool _mentioned = false;

  /// The thread [_audience] was asked for; a failed ask is not asked again for it until the mention is gone.
  ({String? channel})? _asked;

  @override
  void initState() {
    super.initState();
    widget.text.addListener(_changed);
    _changed();
  }

  @override
  void didUpdateWidget(EveryoneAudienceLine old) {
    super.didUpdateWidget(old);
    if (old.text != widget.text) {
      old.text.removeListener(_changed);
      widget.text.addListener(_changed);
    }
    if (old.channel != widget.channel) {
      _timer?.cancel();
      _audience = null;
      _asked = null;
      _changed();
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    widget.text.removeListener(_changed);
    super.dispose();
  }

  void _changed() {
    final mentioned = mentionsEveryone(widget.text.value.text);
    if (mentioned == _mentioned) return;
    _mentioned = mentioned;
    if (!mentioned) {
      // Gone (or sent): the next mention asks again, since the members may have changed.
      _timer?.cancel();
      _asked = null;
      if (_audience != null) setState(() => _audience = null);
      return;
    }
    if (_asked?.channel == widget.channel && _asked != null) return;
    _timer?.cancel();
    _timer = Timer(const Duration(milliseconds: everyoneAudienceDelayMs), _ask);
  }

  Future<void> _ask() async {
    final channel = widget.channel;
    _asked = (channel: channel);
    try {
      final audience = await widget.load(channel);
      if (!mounted || widget.channel != channel) return;
      setState(() => _audience = audience);
    } catch (_) {
      // No line: an older server or a failed read must not get in the way of sending.
    }
  }

  @override
  Widget build(BuildContext context) {
    final audience = _audience;
    if (!_mentioned || audience == null) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Align(
        alignment: Alignment.centerLeft,
        child: Text(everyoneAudienceLine(audience), key: const Key('everyone-audience'), style: TextStyle(fontSize: 12.5, color: Tokens.label2)),
      ),
    );
  }
}
