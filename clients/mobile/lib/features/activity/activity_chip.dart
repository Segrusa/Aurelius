import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../data/activity.dart';
import '../../data/protocol/protocol.dart';
import 'activity_bloc.dart';

/// The bloc when this widget is under the app's providers; a page pumped on its
/// own has none and then simply shows no activity.
ActivityBloc? _blocOf(BuildContext context) {
  try {
    return context.read<ActivityBloc>();
  } on ProviderNotFoundException {
    return null;
  }
}

/// The chip above every tab (ticket #50): how many are busy and wait for a slot;
/// a tap opens the overview. [underBanner]: a banner already covers the status bar.
class ActivityChip extends StatelessWidget {
  const ActivityChip({super.key, this.underBanner = false});

  final bool underBanner;

  @override
  Widget build(BuildContext context) => SafeArea(
        top: !underBanner,
        bottom: false,
        child: BlocBuilder<ActivityBloc, ActivityOverview?>(
          builder: (context, overview) {
            if (overview == null) return const SizedBox(height: 28);
            final waiting = overview.waiting.where(waitsForSlot).length;
            return Align(
              alignment: Alignment.centerLeft,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 2, 16, 2),
                child: ActionChip(
                  key: const ValueKey('activity-chip'),
                  visualDensity: VisualDensity.compact,
                  label: Text('${overview.busy.length} busy · $waiting waiting for a slot', style: const TextStyle(fontSize: 12)),
                  onPressed: () => _showOverview(context),
                ),
              ),
            );
          },
        ),
      );
}

Future<void> _showOverview(BuildContext context) => showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => BlocProvider.value(value: context.read<ActivityBloc>(), child: const _Overview()),
    );

class _Overview extends StatelessWidget {
  const _Overview();

  @override
  Widget build(BuildContext context) => BlocBuilder<ActivityBloc, ActivityOverview?>(
        builder: (context, o) {
          if (o == null) return const SizedBox(height: 120);
          final now = DateTime.now().millisecondsSinceEpoch;
          return SafeArea(
            child: ConstrainedBox(
              constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * .7),
              child: ListView(
                key: const ValueKey('activity-overview'),
                shrinkWrap: true,
                padding: const EdgeInsets.fromLTRB(20, 0, 20, 16),
                children: [
                  _heading('Busy'),
                  if (o.busy.isEmpty) _quiet('Nobody is working right now.'),
                  for (final b in o.busy)
                    _Row(title: b.employeeName + (b.employeePaused == true ? ' (paused, finishes this run)' : ''), lines: [workLabel(b.work), sinceText(b.sinceMs, now)]),
                  const SizedBox(height: 12),
                  _heading('In line'),
                  if (o.waiting.isEmpty) _quiet('Nothing is waiting.'),
                  for (final w in o.waiting)
                    _Row(title: workLabel(w.work), lines: ['${w.employeeName}: ${waitLabel(w.waitsFor, w.employeeName)}', sinceText(w.sinceMs, now)]),
                ],
              ),
            ),
          );
        },
      );

  Widget _heading(String text) => Padding(padding: const EdgeInsets.only(bottom: 6), child: Text(text, style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600)));
  Widget _quiet(String text) => Text(text, style: TextStyle(fontSize: 13, color: Tokens.label2));
}

class _Row extends StatelessWidget {
  const _Row({required this.title, required this.lines});

  final String title;
  final List<String> lines;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(bottom: 8),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(title, style: const TextStyle(fontSize: 14)),
          for (final line in lines) Text(line, style: TextStyle(fontSize: 12, color: Tokens.label2)),
        ]),
      );
}

/// Where a ticket stands now: running, waiting for a slot, paused or something
/// else. Nothing on an older server or when nothing goes on.
class TicketActivityLine extends StatelessWidget {
  const TicketActivityLine({super.key, required this.ticketId, this.padding = EdgeInsets.zero});

  final String ticketId;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    final bloc = _blocOf(context);
    if (bloc == null) return const SizedBox.shrink();
    return BlocBuilder<ActivityBloc, ActivityOverview?>(
      bloc: bloc,
      builder: (context, overview) {
        final found = overview?.tickets.where((t) => t.ticketId == ticketId).firstOrNull;
        if (found == null) return const SizedBox.shrink();
        final color = found.state == 'running' ? Tokens.accentText : (found.state == 'waiting_other' ? Tokens.label2 : Colors.orange.shade700);
        return Padding(
          padding: padding,
          child: Text(ticketActivityLine(found), key: const ValueKey('ticket-activity'), style: TextStyle(fontSize: 12, color: color)),
        );
      },
    );
  }
}
