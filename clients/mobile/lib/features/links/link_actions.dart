import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/net/protocol.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/links.dart';
import '../../data/views.dart' show ErrorCode;
import '../hosts/hosts_bloc.dart';

// What a link does in the app (A5a, ui-ontwerp §6): Copy link, and opening one.
// A link only ever navigates; every way it can fail says so in words.

/// A link for a server this phone is paired with, while switching to it: opened
/// when that host's app is up. In memory only, never on disk (§6.3).
String? pendingAureliusLink;

/// Copy link on a ticket: "Link to #42 copied."
Future<void> copyTicketLink(BuildContext context, String ticketId, int number) async {
  await Clipboard.setData(ClipboardData(text: ticketLinkOf(context.read<AureliusRepository>().host.hostId, ticketId)));
  if (context.mounted) toast(context, 'Link to #$number copied.');
}

/// Copy link on a message.
Future<void> copyMessageLink(BuildContext context, String messageId) async {
  await Clipboard.setData(ClipboardData(text: messageLinkOf(context.read<AureliusRepository>().host.hostId, messageId)));
  if (context.mounted) toast(context, 'Link to message copied.');
}

/// Open [raw] at its place, or say in words why that is not possible (§6.4).
Future<void> openAureliusLink(BuildContext context, String raw) async {
  final link = parseAureliusLink(raw);
  if (link == null) return toast(context, invalidLinkText);
  final repository = context.read<AureliusRepository>();
  if (link.hostId != repository.host.hostId) return _otherServer(context, link, raw);
  if (!repository.linkState.has(messageLocateFeature)) {
    final update = await confirm(context, title: "This server can't open links yet.", body: 'Update the server.', action: 'Updates');
    if (update && context.mounted) context.push('/updates');
    return;
  }
  switch (link) {
    case TicketLink(:final id):
      context.push('/ticket/$id');
    case MessageLink(:final id):
      await openMessageAt(context, repository, id);
  }
}

/// A link for another server: switch to it when this phone knows it (the link
/// opens there), else point at the paired servers.
Future<void> _otherServer(BuildContext context, AureliusLink link, String raw) async {
  final current = context.read<AureliusRepository>().host;
  final hosts = context.read<HostsBloc>();
  final known = hosts.state.hosts.where((h) => h.hostId == link.hostId).firstOrNull;
  if (known == null) {
    final open = await confirm(context, title: "This link is for a server this device isn't paired with.", action: 'Servers');
    if (open && context.mounted) context.push('/hosts');
    return;
  }
  final change = await confirm(context, title: "This link is for ${known.name}. You're on ${current.name}.", action: 'Switch to ${known.name}');
  if (!change) return;
  pendingAureliusLink = raw;
  hosts.add(HostSelected(known.hostId));
}

/// A message's place comes from the server (`message.locate`): its ticket's
/// discussion, or its channel; a reply in a thread opens the thread too, which
/// only the channel of a ticket has.
Future<void> openMessageAt(BuildContext context, AureliusRepository repository, String messageId) async {
  try {
    final at = await repository.messageLocation(messageId);
    if (!context.mounted) return;
    final thread = at.threadRoot;
    if (at.ticketId != null && thread == null) {
      context.push('/ticket/${at.ticketId}?tab=discussion&message=$messageId');
    } else {
      context.push('/channel/${at.conversationId}?message=$messageId${thread == null ? '' : '&thread=$thread'}');
    }
  } on ProtocolException catch (e) {
    if (!context.mounted) return;
    toast(context, switch (e.code) {
      ErrorCode.notFound => 'That message no longer exists.',
      ErrorCode.forbidden => "You don't have access to this message.",
      _ => e.shown,
    });
  } on Object catch (e) {
    if (context.mounted) toast(context, '$e');
  }
}

/// The word for a link once its place is read: `#42`, never an id ("In the UI it stays #42").
final _labels = <String, String>{};

/// Forget them, so a test starts clean.
@visibleForTesting
void forgetLinkLabels() => _labels.clear();

/// What a link says until it is read, or on a server that cannot read it.
const _unreadLabel = 'Aurelius link';

/// Provides the words for the links written out in [text] to [builder]: read
/// from the server once per link, for all messages of this run of the app.
class AureliusLinkLabels extends StatefulWidget {
  const AureliusLinkLabels({super.key, required this.text, required this.builder});

  final String text;
  final Widget Function(BuildContext context, String Function(AureliusLink) label) builder;

  @override
  State<AureliusLinkLabels> createState() => _AureliusLinkLabelsState();
}

class _AureliusLinkLabelsState extends State<AureliusLinkLabels> {
  /// Could not be read now: "Unavailable link" until it can (§6.2).
  final _failed = <String>{};
  final _asked = <String>{};

  @override
  void initState() {
    super.initState();
    _read();
  }

  @override
  void didUpdateWidget(AureliusLinkLabels oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.text != widget.text) _read();
  }

  static String _key(AureliusLink link) => '${link.hostId}/${link.id}';

  void _read() {
    final repository = context.read<AureliusRepository>();
    if (!repository.linkState.has(messageLocateFeature)) return;
    for (final found in bareAureliusLinks(widget.text)) {
      final link = found.link;
      final key = _key(link);
      if (link.hostId != repository.host.hostId || _labels.containsKey(key) || !_asked.add(key)) continue;
      _labelOf(repository, link).then((label) {
        _labels[key] = label;
        if (mounted) setState(() {});
      }).catchError((Object _) {
        if (mounted) setState(() => _failed.add(key));
      });
    }
  }

  static Future<String> _labelOf(AureliusRepository repository, AureliusLink link) async => switch (link) {
        TicketLink(:final id) => '#${(await repository.ticket(id)).ticket.number}',
        MessageLink(:final id) => switch ((await repository.messageLocation(id)).ticketNumber) { final number? => 'Message in #$number', null => 'Message' },
      };

  String _label(AureliusLink link) {
    final key = _key(link);
    return _labels[key] ?? (_failed.contains(key) ? 'Unavailable link' : _unreadLabel);
  }

  @override
  Widget build(BuildContext context) => widget.builder(context, _label);
}
