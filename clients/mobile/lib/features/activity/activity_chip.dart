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
