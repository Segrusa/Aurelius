import 'package:flutter_test/flutter_test.dart';
import 'package:aurelius_mobile/features/team/channel_screen.dart';
import 'package:aurelius_mobile/features/team/channels_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Opens a channel as a link does (ticket #71 took the Team tab and its
/// channel list away): a button per channel, keyed `channel-<id>`, that pushes
/// its screen with the reader's read_through.
class ChannelOpener extends StatelessWidget {
  const ChannelOpener({super.key});

  /// The [ChannelsBloc] above the opener.
  static ChannelsBloc channelsOf(WidgetTester tester) => tester.element(find.byType(ChannelOpener, skipOffstage: false)).read<ChannelsBloc>();

  @override
  Widget build(BuildContext context) {
    final channels = context.watch<ChannelsBloc>().state.channels;
    return Scaffold(
      body: ListView(children: [
        for (final c in channels)
          TextButton(
            key: ValueKey('channel-${c.conversationId}'),
            onPressed: () => Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => ChannelScreen(channel: c, readThrough: c.readThrough))),
            child: Text(c.name),
          ),
      ]),
    );
  }
}
