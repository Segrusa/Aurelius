import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/discussion.dart';
import '../../data/views.dart';
import '../chat/chat_bloc.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';
import 'channels_bloc.dart';

/// The company's group chat (Team): the user and every employee. `@Name`
/// asks those employees; without one the head of the company answers. With a
/// server of Phase 2 it has channels (MU2-08): General and one per project,
/// picked from a pill at the top (ui-ontwerp Fase 2 §3).
class TeamPage extends StatefulWidget {
  const TeamPage({super.key});

  @override
  State<TeamPage> createState() => _TeamPageState();
}

class _TeamPageState extends State<TeamPage> {
  /// The open channel's conversation; null: General.
  String? _channel;

  ChannelsState _channels(BuildContext context) {
    try {
      return context.watch<ChannelsBloc>().state;
    } on ProviderNotFoundException {
      return const ChannelsState();
    }
  }

  Future<void> _pick(ChannelsState channels) async {
    final picked = await showChannelSheet(context, channels, current: _channel);
    if (picked == null || !mounted) return;
    setState(() => _channel = picked.projectId == null ? null : picked.conversationId);
    context.read<ChannelsBloc>().add(const ChannelsRefreshed());
  }

  void _readChannels() {
    if (!mounted) return;
    try {
      context.read<ChannelsBloc>().add(const ChannelsRefreshed());
    } on ProviderNotFoundException {
      // No channels here.
    }
  }

  @override
  Widget build(BuildContext context) {
    final company = context.read<CompanyBloc>();
    final channels = _channels(context);
    final open = channels.supported ? (channels.byId(_channel) ?? channels.general) : null;
    // A channel that went away (its project deleted): back to General.
    final channel = open?.projectId == null ? null : open!.conversationId;
    final others = channels.channels.where((c) => c.conversationId != open?.conversationId).fold(0, (n, c) => n + c.unread);
    final label = open == null ? 'Team chat' : channelLabel(open);
    return Scaffold(
      appBar: GlassAppBar(title: const Text('Team')),
      body: Column(children: [
        if (open != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 6),
            child: Row(children: [
              Semantics(
                button: true,
                label: 'Channel $label. Change channel',
                excludeSemantics: true,
                child: ActionChip(
                  key: const ValueKey('channel-picker'),
                  label: Row(mainAxisSize: MainAxisSize.min, children: [
                    Text(label, style: const TextStyle(fontWeight: FontWeight.w600)),
                    const SizedBox(width: 4),
                    const Icon(CupertinoIcons.chevron_down, size: 14),
                  ]),
                  onPressed: () => _pick(channels),
                ),
              ),
              const Spacer(),
              if (others > 0) Text('● $others unread', key: const ValueKey('channels-unread'), style: TextStyle(color: Tokens.label2, fontSize: 13)),
            ]),
          ),
        Expanded(
          child: BlocProvider(
            key: ValueKey('team-${channel ?? 'general'}'),
            // The employee at work on a run: who writes a streaming answer.
            create: (_) => ChatBloc.group(
              context.read<AureliusRepository>(),
              runAuthor: (runId) => runAuthorOf(company.state, runId),
              channel: channel,
              // The server marked it read: the badge and the counts follow.
              onRead: _readChannels,
            ),
            child: ChatShownWatcher(
              child: ChatView(
                key: ValueKey('team-chat-${channel ?? 'general'}'),
                emptyTitle: 'Talk to your team',
                emptyDetail: open?.projectId == null
                    ? 'Write @Name to ask someone; otherwise the head of the company answers.'
                    : "Write @Name to ask someone; otherwise the project's dispatcher answers.",
                composer: !(open?.archived ?? false),
                readOnlyNote: open?.archived ?? false ? 'This project is archived: its channel can only be read.' : null,
                hint: open?.projectId == null ? null : 'Message $label · @Name asks someone',
                source: TicketSource(from: 'Team chat · ${open == null ? '#general' : label}', projectId: open?.projectId),
              ),
            ),
          ),
        ),
      ]),
    );
  }
}

/// Tells the Team chat's [ChatBloc] whether it is on screen: the shell turns
/// the tickers of hidden tabs (and of pages under another one) off, and a
/// phone in the background shows nothing either. Only a chat on screen is read,
/// for reading a channel marks it read on the server.
class ChatShownWatcher extends StatefulWidget {
  const ChatShownWatcher({super.key, required this.child});

  final Widget child;

  @override
  State<ChatShownWatcher> createState() => _ChatShownWatcherState();
}

class _ChatShownWatcherState extends State<ChatShownWatcher> with WidgetsBindingObserver {
  bool? _shown;
  bool _ticking = true;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  bool get _foreground => switch (WidgetsBinding.instance.lifecycleState) {
        null || AppLifecycleState.resumed => true,
        _ => false,
      };

  void _tell() {
    final shown = _ticking && _foreground;
    if (shown == _shown) return;
    _shown = shown;
    context.read<ChatBloc>().add(ChatShown(shown));
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _ticking = TickerMode.valuesOf(context).enabled;
    _tell();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) => _tell();

  @override
  Widget build(BuildContext context) => widget.child;
}

/// The sheet "Channels" (ui-ontwerp Fase 2 §3): General, then Projects and the
/// archived ones (read-only), each with its unread count and an @ for mentions.
Future<ChannelView?> showChannelSheet(BuildContext context, ChannelsState channels, {String? current}) {
  final groups = channelGroups(channels.channels);
  Widget row(BuildContext sheet, ChannelView c) {
    final selected = c.projectId == null ? current == null : c.conversationId == current;
    return Semantics(
      label: channelSemantics(c),
      button: true,
      selected: selected,
      excludeSemantics: true,
      child: ListTile(
        key: ValueKey('channel-${c.conversationId}'),
        title: Text(channelLabel(c), style: TextStyle(fontWeight: c.unread > 0 ? FontWeight.w700 : null, color: c.archived ? Tokens.label2 : null)),
        trailing: Row(mainAxisSize: MainAxisSize.min, children: [
          if (c.mentions > 0)
            Container(
              key: ValueKey('channel-mentions-${c.conversationId}'),
              padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
              decoration: BoxDecoration(color: Tokens.accent, borderRadius: BorderRadius.circular(999)),
              child: Text('@ ${c.mentions}', style: TextStyle(color: Tokens.accentInk, fontSize: 12, fontWeight: FontWeight.w600)),
            ),
          if (c.unread > 0) ...[
            const SizedBox(width: 6),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
              decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(999)),
              child: Text('${c.unread}', style: TextStyle(color: Tokens.label2, fontSize: 12)),
            ),
          ],
          if (selected) ...[const SizedBox(width: 6), Icon(Icons.check, color: Tokens.accentText, size: 18)],
        ]),
        subtitle: c.lastActivityMs == null ? null : Text(Fmt.ago(c.lastActivityMs!), style: TextStyle(color: Tokens.label2, fontSize: 12)),
        onTap: () => Navigator.pop(sheet, c),
      ),
    );
  }

  return showModalBottomSheet<ChannelView>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheet) => SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.75),
        child: ListView(shrinkWrap: true, children: [
          const Padding(padding: EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Channels')),
          for (final c in groups.general) row(sheet, c),
          if (groups.projects.isNotEmpty) ...[
            const Padding(padding: EdgeInsets.fromLTRB(16, 12, 16, 4), child: Eyebrow('Projects')),
            for (final c in groups.projects) row(sheet, c),
          ],
          if (groups.archived.isNotEmpty)
            ExpansionTile(
              key: const ValueKey('channels-archived'),
              title: const Eyebrow('Archived'),
              children: [for (final c in groups.archived) row(sheet, c)],
            ),
        ]),
      ),
    ),
  );
}

/// The name of the employee whose agent runs [runId], if one does now.
String? runAuthorOf(CompanyState company, String runId) =>
    company.employees.where((m) => m.agents.any((a) => a.runId == runId)).firstOrNull?.name;
