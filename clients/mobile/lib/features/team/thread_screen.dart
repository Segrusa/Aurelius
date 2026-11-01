import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../data/aurelius_repository.dart';
import '../../data/channels.dart';
import '../../data/discussion.dart';
import '../../data/views.dart';
import '../chat/chat_bloc.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';
import 'channels_bloc.dart';
import 'team_page.dart';

/// One thread of a Team chat channel (AUR-CHAT-002 §5, feature
/// `chat_threads`): its own pushed screen "Thread · #channel" with the root
/// message on top, its replies and a composer that replies in the thread
/// (`thread_parent`). Read-only when its channel is closed (`read_only`).
class ThreadScreen extends StatelessWidget {
  const ThreadScreen({super.key, required this.channel, required this.root});

  /// The channel as it was when the thread opened; the list's newer view of it wins.
  final ChannelView channel;
  final MessageView root;

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
    return Scaffold(
      appBar: GlassAppBar(title: Text(threadTitle(live), key: const ValueKey('thread-title'), maxLines: 1, overflow: TextOverflow.ellipsis, style: Tokens.title(size: 16))),
      body: BlocProvider(
        key: ValueKey('thread-${root.id}'),
        create: (_) => ChatBloc.thread(
          context.read<AureliusRepository>(),
          threadRoot: root.id,
          runAuthor: (runId) => runAuthorOf(company.state, runId),
          // General is written without a channel, as before.
          channel: general ? null : id,
          channelTicket: ticketId,
          onRead: () => _readChannels(context),
        ),
        child: BlocListener<ChannelsBloc, ChannelsState>(
          // The ticket left Done (or the project came back): replying works again.
          listenWhen: (a, b) => (a.byId(id)?.archived ?? false) && b.byId(id)?.archived == false,
          listener: (context, _) => context.read<ChatBloc>().add(const ChatRefreshed()),
          child: ChatShownWatcher(
            child: ChatView(
              key: ValueKey('thread-chat-${root.id}'),
              emptyTitle: 'No replies yet',
              composer: !live.archived,
              readOnlyNote: live.archived ? channelReadOnlyNote(live) : null,
              hint: threadHint,
              // A reply being typed is the thread's, never the channel's draft.
              draftSlot: 'thread:${root.id}',
              channel: ChannelChatOptions(
                closedNote: channelReadOnlyNote(live),
                threads: true,
                reactions: context.read<AureliusRepository>().linkState.has(chatReactionsFeature),
                thread: true,
              ),
              source: TicketSource(from: 'Team chat · $label', projectId: live.projectId),
            ),
          ),
        ),
      ),
    );
  }
}
