import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

/// The subagents among what runs, that work in `conversation`.
List<AgentView> subagentsIn(Iterable<AgentView> agents, String conversationId) =>
    agents.where((a) => a.kind == 'subagent' && a.conversationId == conversationId).toList();

/// How many subagents a conversation runs ("● 2 subagents"), as on the
/// desktop. Tap for what each one was asked and its latest step. Asks at
/// open, on reconnect, when the answer starts or ends ([running]) and when
/// a stream append shows a new or finished step (also a subagent's), at most
/// once per [settle]; hidden when none run. An older server (before 1.6)
/// only tells it for an [employee] conversation.
class SubagentChip extends StatefulWidget {
  const SubagentChip({super.key, required this.conversationId, required this.running, this.employee = false});

  final String conversationId;
  final bool running;
  final bool employee;

  /// The least time between two asks caused by the stream.
  static const settle = Duration(seconds: 1);

  @override
  State<SubagentChip> createState() => _SubagentChipState();
}

class _SubagentChipState extends State<SubagentChip> {
  /// Shared with an open sheet, so it follows along.
  final _agents = ValueNotifier<List<AgentView>>(const []);
  StreamSubscription<void>? _online;
  StreamSubscription<StreamAppend>? _appends;

  /// Running while the stream may not ask again; [_pending] asks when it ends.
  Timer? _cooldown;
  bool _pending = false;

  /// Steps and finished steps of the answer, as the last append told them.
  (int, int)? _mark;
  bool _loading = false;
  bool _again = false;
  bool _unsupported = false;

  @override
  void initState() {
    super.initState();
    final repository = context.read<AureliusRepository>();
    _online = repository.online.listen((_) => _load());
    _appends = repository.appends.where((a) => a.conversationId == widget.conversationId).listen(_appended);
    _load();
  }

  @override
  void didUpdateWidget(SubagentChip old) {
    super.didUpdateWidget(old);
    // Once more when the answer ends: its subagents are done then.
    if (old.running != widget.running) _load();
  }

  @override
  void dispose() {
    _cooldown?.cancel();
    _online?.cancel();
    _appends?.cancel();
    // Not disposed: a sheet that is still open may listen to it.
    super.dispose();
  }

  /// The server sends the activity with an append only when it changed;
  /// a subagent's step lands there too.
  void _appended(StreamAppend append) {
    final activity = append.activity;
    if (activity == null) return;
    final mark = (activity.length, activity.where((a) => a.ok != null).length);
    if (mark == _mark) return;
    _mark = mark;
    _soon();
  }

  void _soon() {
    if (_cooldown?.isActive ?? false) {
      _pending = true;
      return;
    }
    _load();
    _cooldown = Timer(SubagentChip.settle, () {
      if (!_pending || !mounted) return;
      _pending = false;
      _soon();
    });
  }

  Future<void> _load() async {
    final repository = context.read<AureliusRepository>();
    if (_unsupported || !repository.linkState.isOnline) return;
    if (_loading) {
      // Asked while asking: ask once more after, so no step is missed.
      _again = true;
      return;
    }
    if (WidgetsBinding.instance.lifecycleState case final s? when s != AppLifecycleState.resumed) return;
    _loading = true;
    var retry = false;
    try {
      final agents = await repository.subagents(widget.conversationId);
      if (mounted && !_same(agents, _agents.value)) {
        _agents.value = agents;
        setState(() {});
      }
    } on ProtocolException {
      _unsupported = true;
    } catch (_) {
      // Offline for a moment: keep what is shown.
    } finally {
      _loading = false;
    }
    if (_again) {
      _again = false;
      retry = true;
    }
    if (retry && mounted) await _load();
  }

  static bool _same(List<AgentView> a, List<AgentView> b) => a.length == b.length && Iterable.generate(a.length).every((i) => a[i].same(b[i]));

  @override
  Widget build(BuildContext context) {
    final n = _agents.value.length;
    if (n == 0) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.fromLTRB(0, 6, 12, 0),
      child: Material(
        color: Tokens.surface,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Tokens.radius), side: BorderSide(color: Tokens.border)),
        child: InkWell(
          key: const Key('subagent-chip'),
          borderRadius: BorderRadius.circular(Tokens.radius),
          onTap: () => SubagentSheet.open(context, _agents),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                const StatusDot(pulse: true, size: 6),
                const SizedBox(width: 7),
                Text(n == 1 ? '1 subagent' : '$n subagents', style: TextStyle(fontSize: 12.5, color: Tokens.text2)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// What the running subagents were asked and their latest step. Closes by
/// itself when the last one is done.
class SubagentSheet extends StatefulWidget {
  const SubagentSheet({super.key, required this.agents});

  final ValueListenable<List<AgentView>> agents;

  static Future<void> open(BuildContext context, ValueListenable<List<AgentView>> agents) => showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        builder: (_) => SubagentSheet(agents: agents),
      );

  @override
  State<SubagentSheet> createState() => _SubagentSheetState();
}

class _SubagentSheetState extends State<SubagentSheet> {
  /// The times tick while the sheet is open.
  late final Timer _tick;
  final _expanded = <String>{};

  @override
  void initState() {
    super.initState();
    _tick = Timer.periodic(const Duration(seconds: 1), (_) => setState(() {}));
    widget.agents.addListener(_changed);
  }

  @override
  void dispose() {
    _tick.cancel();
    widget.agents.removeListener(_changed);
    super.dispose();
  }

  void _changed() {
    if (!mounted) return;
    if (widget.agents.value.isEmpty) {
      Navigator.of(context).maybePop();
    } else {
      setState(() {});
    }
  }

  @override
  Widget build(BuildContext context) {
    final agents = widget.agents.value;
    final now = DateTime.now().millisecondsSinceEpoch;
    return SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.7),
        child: ListView(
          shrinkWrap: true,
          padding: const EdgeInsets.only(bottom: 12),
          children: [
            const Padding(padding: EdgeInsets.fromLTRB(16, 16, 16, 4), child: Eyebrow('Subagents at work')),
            for (final (i, a) in agents.indexed) _row(a, i, now),
          ],
        ),
      ),
    );
  }

  Widget _row(AgentView a, int i, int now) {
    final key = '${a.sinceMs}-${a.name}';
    final prompt = (a.prompt ?? '').trim();
    final folded = prompt.split('\n').where((l) => l.trim().isNotEmpty).take(3).join('\n');
    final more = folded.length < prompt.length;
    final open = _expanded.contains(key);
    return InkWell(
      key: ValueKey('subagent-$key'),
      onTap: more ? () => setState(() => open ? _expanded.remove(key) : _expanded.add(key)) : null,
      child: Container(
        padding: const EdgeInsets.fromLTRB(16, 10, 16, 10),
        decoration: i == 0 ? null : BoxDecoration(border: Border(top: BorderSide(color: Tokens.border))),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Flexible(child: Text(a.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 14, fontWeight: FontWeight.w500, color: Tokens.text))),
                if (a.detail.isNotEmpty) ...[
                  const SizedBox(width: 8),
                  Expanded(child: Text(a.detail, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12.5, color: Tokens.muted))),
                ] else
                  const Spacer(),
                const SizedBox(width: 8),
                Text(elapsed(now - a.sinceMs), style: TextStyle(fontSize: 12, color: Tokens.muted)),
              ],
            ),
            const SizedBox(height: 3),
            Text(
              doingLine(a),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 13, color: Tokens.accentText),
            ),
            if (prompt.isNotEmpty) ...[
              const SizedBox(height: 4),
              Text(open ? prompt : (more ? '$folded …' : folded), style: TextStyle(fontSize: 12.5, height: 1.4, color: Tokens.text2)),
            ],
          ],
        ),
      ),
    );
  }
}

/// "↳ Grep · x · 3 steps", or "↳ starting…" before its first step.
String doingLine(AgentView a) => switch (a.doing) {
      final doing? => '↳ $doing · ${a.steps} ${a.steps == 1 ? 'step' : 'steps'}',
      null => '↳ starting…',
    };

/// "12s", "1m 12s", "1h 03m".
String elapsed(int ms) {
  final s = (ms < 0 ? 0 : ms) ~/ 1000;
  if (s < 60) return '${s}s';
  if (s < 3600) return '${s ~/ 60}m ${(s % 60).toString().padLeft(2, '0')}s';
  return '${s ~/ 3600}h ${(s ~/ 60 % 60).toString().padLeft(2, '0')}m';
}
