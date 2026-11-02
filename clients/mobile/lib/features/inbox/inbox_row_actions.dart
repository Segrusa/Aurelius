import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../data/aurelius_repository.dart';
import '../../data/links.dart';
import '../../data/views.dart';
import '../board/ticket_actions.dart';
import '../links/link_actions.dart';
import 'inbox_bloc.dart';
import 'inbox_rows.dart';

/// A tap on a row: straight to the place, no screen in between (§4.5). Opening
/// is what makes it read. A row that names no place (an unknown kind) only reads.
Future<void> openInboxRow(BuildContext context, NotificationView row) async {
  context.read<InboxBloc>().add(InboxRowOpened(row));
  final message = row.messageId;
  final repository = context.read<AureliusRepository>();
  // A reply in a thread has its own place; only the server knows it.
  if (message != null && repository.linkState.has(messageLocateFeature)) return openMessageAt(context, repository, message);
  final place = inboxPlace(reason: row.reason, ticketId: row.ticketId, conversationId: row.conversationId, messageId: message);
  if (place != null) context.push(place);
}

/// The `+n` of a ticket's row: its Activity (freeze §4). It opens the row as well.
void openInboxRowActivity(BuildContext context, NotificationView row) {
  context.read<InboxBloc>().add(InboxRowOpened(row));
  context.push(activityOf(row.ticketId!));
}

/// A long press on a row (§4.5). Opening a row reads it, so there is no Mark read
/// here. Unfollow goes to the server, which refuses it for the
/// Responsible and says why.
Future<void> showInboxRowMenu(BuildContext context, NotificationView row, {required bool follow, required bool links}) async {
  final ticket = row.ticketId;
  final number = row.ticketNumber;
  final message = row.messageId;
  final items = <(String, IconData, Future<void> Function())>[
    ('Open', CupertinoIcons.arrow_up_right_square, () => openInboxRow(context, row)),
    if (follow && ticket != null && number != null) ('Unfollow', CupertinoIcons.bell_slash, () => followTicketById(context, ticket, number, on: false)),
    if (links && message != null)
      ('Copy link', CupertinoIcons.link, () => copyMessageLink(context, message))
    else if (links && ticket != null && number != null)
      ('Copy link', CupertinoIcons.link, () => copyTicketLink(context, ticket, number)),
  ];
  final picked = await showModalBottomSheet<int>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheet) => SafeArea(
      child: SingleChildScrollView(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          if (row.place.isNotEmpty) Padding(padding: const EdgeInsets.fromLTRB(20, 0, 20, 8), child: Text(row.place, maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600))),
          for (final (i, item) in items.indexed)
            ListTile(key: ValueKey('inbox-row-menu-${item.$1}'), leading: Icon(item.$2), title: Text(item.$1), onTap: () => Navigator.pop(sheet, i)),
        ]),
      ),
    ),
  );
  if (picked != null && context.mounted) await items[picked].$3();
}
