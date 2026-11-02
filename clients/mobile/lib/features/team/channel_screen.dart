import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/channels.dart';
import '../../data/discussion.dart';
import '../../data/tickets.dart' show responsibleLabel;
import '../../data/views.dart';
import '../chat/chat_bloc.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';
import 'channel_list.dart';
import 'channel_ticket_bloc.dart';
import 'channels_bloc.dart';
import 'team_page.dart';
import 'thread_screen.dart';

/// One channel of the Slack-style Team chat (AUR-CHAT-001 §2–§4, §6): its
/// header, the flat message list and the composer, or the read-only bar when
/// the channel is archived. Pushed over the channel list on a phone, beside it
/// on a tablet. [readThrough] is the reader's last read before it opened:
/// opening it marks it read, and "New messages" stays where it was.
class ChannelScreen extends StatelessWidget {
  const ChannelScreen({super.key, required this.channel, this.readThrough, this.onOpen, this.revealMessage, this.revealThread});

  /// The channel as it was when opened; the list's newer view of it wins.
  final ChannelView channel;
  final int? readThrough;

  /// Opens another channel, as a tap in the list does (a line in General links to a ticket's).
  final ValueChanged<ChannelView>? onOpen;

  /// A link opened here (A5a): the message to scroll to.
  final String? revealMessage;

  /// That message is a reply: its thread's root, opened as well.
  final String? revealThread;

  /// Opens the channel [conversationId] a line links to; false when the list does not know it (yet).
  bool _openLinked(BuildContext context, String conversationId) {
    final found = context.read<ChannelsBloc>().state.byId(conversationId);
    if (found == null || onOpen == null) return false;
    onOpen!(found);
    return true;
  }

  /// Pushes the thread of [root] (AUR-CHAT-002 §5); [context] is under the
  /// channel's [ChatBloc], which reads the root's counters and repliers again
  /// when the thread closes.
  void _openThread(BuildContext context, ChannelView live, MessageView root) {
    final bloc = context.read<ChatBloc>();
    Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => ThreadScreen(channel: live, root: root, revealMessage: root.id == revealThread ? revealMessage : null))).then((_) {
      if (!bloc.isClosed) bloc.add(const ChatRefreshed());
    });
  }

  void _readChannels(BuildContext context) {
    if (!context.mounted) return;
    try {
      context.read<ChannelsBloc>().add(const ChannelsRefreshed());
    } on ProviderNotFoundException {
      // No channels here.
    }
  }

  @override
  Widget build(BuildContext context) {
    final id = channel.conversationId;
    final live = context.select<ChannelsBloc, ChannelView?>((b) => b.state.byId(id)) ?? channel;
    final ticketId = isTicketChannel(live) ? live.ticketId : null;
    final general = live.projectId == null && !isTicketChannel(live);
    final company = context.read<CompanyBloc>();
    final label = channelLabel(live);
    // Threads and reactions only when the server has them (AUR-CHAT-002); otherwise as in phase 1.
    final link = context.read<AureliusRepository>().linkState;
    final threads = link.has(chatThreadsFeature);
    final reactions = link.has(chatReactionsFeature);
    Widget page = Scaffold(
      appBar: ChannelAppBar(channel: live),
      body: BlocProvider(
        key: ValueKey('team-$id'),
        create: (_) => ChatBloc.group(
          context.read<AureliusRepository>(),
          runAuthor: (runId) => runAuthorOf(company.state, runId),
          // General is read without a channel, as before.
          channel: general ? null : id,
          channelTicket: ticketId,
          onRead: () => _readChannels(context),
        ),
        child: MultiBlocListener(
          listeners: [
            // The ticket left Done (or the project came back): writing works again (§8 Reopen).
            BlocListener<ChannelsBloc, ChannelsState>(
              listenWhen: (a, b) => (a.byId(id)?.archived ?? false) && b.byId(id)?.archived == false,
              listener: (context, _) => context.read<ChatBloc>().add(const ChatReopened()),
            ),
            // The ticket went to Done: its closed lines show at once, under the read-only bar.
            BlocListener<ChannelsBloc, ChannelsState>(
              listenWhen: (a, b) => a.byId(id)?.archived == false && (b.byId(id)?.archived ?? false),
              listener: (context, _) => context.read<ChatBloc>().add(const ChatRefreshed()),
            ),
          ],
          child: ChatShownWatcher(
            child: ChatView(
              key: ValueKey('team-chat-$id'),
              revealMessage: revealThread ?? revealMessage,
              emptyTitle: isTicketChannel(live) ? 'No messages yet' : 'Talk to your team',
              emptyDetail: general
                  ? 'Write @Name to ask someone; otherwise the head of the company answers.'
                  : isTicketChannel(live)
                      ? 'Write @Name to ask someone about this ticket.'
                      : "Write @Name to ask someone; otherwise the project's dispatcher answers.",
              composer: !live.archived,
              readOnlyNote: live.archived ? channelReadOnlyNote(live) : null,
              readOnlyAction: ticketId == null
                  ? null
                  : TextButton(key: const ValueKey('channel-read-only-open'), onPressed: () => context.push('/ticket/$ticketId'), child: const Text('Open ticket')),
              hint: channelHint(live),
              channel: ChannelChatOptions(
                readThrough: readThrough,
                closedNote: channelReadOnlyNote(live),
                openChannel: (linked) => _openLinked(context, linked),
                threads: threads,
                reactions: reactions,
                openThread: threads ? (inner, root) => _openThread(inner, live, root) : null,
                revealThread: revealThread != null,
              ),
              source: TicketSource(from: 'Team chat · $label', projectId: live.projectId),
            ),
          ),
        ),
      ),
    );
    if (ticketId != null) {
      page = BlocProvider(
        key: ValueKey('channel-ticket-$ticketId'),
        create: (_) => ChannelTicketBloc(context.read<AureliusRepository>(), ticketId),
        child: _HeaderShownWatcher(child: page),
      );
    }
    return page;
  }
}

/// Tells the header's [ChannelTicketBloc] whether it is on screen (a hidden
/// tab or a page above it turns its tickers off).
class _HeaderShownWatcher extends StatefulWidget {
  const _HeaderShownWatcher({required this.child});

  final Widget child;

  @override
  State<_HeaderShownWatcher> createState() => _HeaderShownWatcherState();
}

class _HeaderShownWatcherState extends State<_HeaderShownWatcher> {
  bool? _shown;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final shown = TickerMode.valuesOf(context).enabled;
    if (shown == _shown) return;
    _shown = shown;
    context.read<ChannelTicketBloc>().add(ChannelTicketShown(shown));
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// The channel's header (§3, §6): `#42-login-per…`, and for a ticket's channel
/// a second line with its status and assignees, and Open to the ticket page.
class ChannelAppBar extends StatelessWidget implements PreferredSizeWidget {
  const ChannelAppBar({super.key, required this.channel});

  final ChannelView channel;

  @override
  Size get preferredSize => const Size.fromHeight(56);

  @override
  Widget build(BuildContext context) {
    final ticketId = isTicketChannel(channel) ? channel.ticketId : null;
    final title = Text(channelLabel(channel), key: const ValueKey('channel-title'), maxLines: 1, overflow: TextOverflow.ellipsis, style: Tokens.title(size: 16));
    return GlassAppBar(
      title: ticketId == null ? title : Column(mainAxisSize: MainAxisSize.min, children: [title, _TicketLine(channel: channel)]),
      actions: [
        if (ticketId != null)
          TextButton(key: const ValueKey('channel-open-ticket'), onPressed: () => context.push('/ticket/$ticketId'), child: const Text('Open')),
      ],
    );
  }
}

/// "[Review] · Forge, Schema": the ticket's column and who works on it (§3).
class _TicketLine extends StatelessWidget {
  const _TicketLine({required this.channel});

  final ChannelView channel;

  @override
  Widget build(BuildContext context) {
    final state = context.watch<ChannelTicketBloc>().state;
    final column = state.ticket?.column ?? channel.ticketColumn;
    final names = state.assignees;
    // Who follows it up, after who works on it (§1.4); a server without a responsible sends none.
    final responsible = state.ticket == null ? null : responsibleLabel(state.ticket!, me: context.read<AureliusRepository>().linkState.user?.id);
    return Semantics(
      label: [
        if (column != null) 'Status ${columnName(column)}',
        if (state.ticket != null) state.ticket!.title,
        if (names.isNotEmpty) 'assigned to ${names.join(', ')}',
        if (responsible != null) '$responsible responsible',
      ].join(', '),
      excludeSemantics: true,
      child: Row(key: const ValueKey('channel-ticket-line'), mainAxisSize: MainAxisSize.min, children: [
        if (column != null) ChannelStatusPill(column: column),
        if (names.isNotEmpty) ...[
          Text(' · ', style: TextStyle(fontSize: 12, color: Tokens.label2)),
          for (final n in names.take(2)) Padding(padding: const EdgeInsets.only(right: 3), child: Monogram(n.characters.first.toUpperCase(), size: 16)),
          Flexible(child: Text(assigneesLine(names), maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12, color: Tokens.label2))),
        ],
        if (responsible != null) ...[
          Text(' · ', style: TextStyle(fontSize: 12, color: Tokens.label2)),
          Monogram(responsible.characters.first.toUpperCase(), size: 16, member: true, dimmed: state.ticket!.responsibleDisabled ?? false),
          const SizedBox(width: 3),
          Flexible(child: Text('$responsible · responsible', key: const ValueKey('channel-ticket-responsible'), maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12, color: Tokens.label2))),
        ],
      ]),
    );
  }
}
