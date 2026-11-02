import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import 'channel_screen.dart';
import 'channels_bloc.dart';

/// `/channel/:cid` (A5a): where a link to a message in a channel lands, on
/// [message] and in its [thread]. Reads the channels first when they are not known yet.
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
    if (channel != null) return ChannelScreen(channel: channel, revealMessage: widget.message, revealThread: widget.thread);
    final read = channels.channels.isNotEmpty || channels.error != null;
    return Scaffold(
      appBar: const GlassAppBar(title: Text('Channel')),
      body: _askedAgain && read
          ? EmptyState('This channel is not available.', key: const ValueKey('channel-link-unknown'), detail: channels.error, action: FilledButton(onPressed: () => context.go('/board'), child: const Text('Go to Board')))
          : const Center(child: CircularProgressIndicator()),
    );
  }
}
