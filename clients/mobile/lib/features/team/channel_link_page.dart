import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/channels.dart';
import '../../data/protocol/protocol.dart';
import 'channel_screen.dart';
import 'channels_bloc.dart';

/// Where a link to the channel [c] opens instead of the channel (ticket #71):
/// a ticket's channel is its discussion on the ticket's page, at [message]
/// when there is one. Null: the channel itself (General, a project's).
String? channelLinkPlace(ChannelView c, {String? message}) {
  final ticket = c.ticketId;
  if (!isTicketChannel(c) || ticket == null) return null;
  return message == null ? '/ticket/$ticket?tab=discussion' : '/ticket/$ticket?tab=discussion&message=$message';
}

/// `/channel/:cid` (A5a): where a link to a message in a channel lands, on
/// [message] and in its [thread]. Reads the channels first when they are not
/// known yet. A ticket's channel opens the ticket instead ([channelLinkPlace]).
class ChannelLinkPage extends StatefulWidget {
  const ChannelLinkPage({super.key, required this.conversationId, this.message, this.thread});

  final String conversationId;
  final String? message;
  final String? thread;

  @override
  State<ChannelLinkPage> createState() => _ChannelLinkPageState();
}

class _ChannelLinkPageState extends State<ChannelLinkPage> {
  bool _askedAgain = false;

  /// The ticket took this page's place once; a rebuild before it did must not do it again.
  bool _left = false;

  @override
  void initState() {
    super.initState();
    final channels = context.read<ChannelsBloc>();
    if (channels.state.byId(widget.conversationId) == null) {
      channels.add(const ChannelsRefreshed());
      _askedAgain = true;
    }
  }

  @override
  Widget build(BuildContext context) {
    final channels = context.watch<ChannelsBloc>().state;
    final channel = channels.byId(widget.conversationId);
    final ticket = channel == null ? null : channelLinkPlace(channel, message: widget.message);
    if (ticket != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_left || !context.mounted) return;
        _left = true;
        context.pushReplacement(ticket);
      });
      return const Scaffold(body: Center(child: CircularProgressIndicator()));
    }
    if (channel != null) return ChannelScreen(channel: channel, revealMessage: widget.message, revealThread: widget.thread);
    final read = channels.channels.isNotEmpty || channels.error != null;
    return Scaffold(
      appBar: const GlassAppBar(title: Text('Channel')),
      body: _askedAgain && read
          ? EmptyState('This channel is not available.', key: const ValueKey('channel-link-unknown'), detail: channels.error, action: FilledButton(onPressed: () => context.go('/sessions?tab=tickets'), child: const Text('Open tickets')))
          : const Center(child: CircularProgressIndicator()),
    );
  }
}
