import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/channels.dart';
import '../../data/discussion.dart';
import '../../data/views.dart';
import 'channels_bloc.dart';

/// The Team tab's first screen with a server of `ticket_channels` (AUR-CHAT-001
/// §1, §6): a search field, then Channels, Tickets (open, latest activity
/// first) and Archived (folded), Tickets and Archived with "Show more" after
/// 15 rows. A row is bold when unread, with an `@ n` pill for mentions and the
/// unread count; the open channel ([selected], side by side) has no badge.
class ChannelList extends StatefulWidget {
  const ChannelList({super.key, required this.onOpen, this.selected});

  final ValueChanged<ChannelView> onOpen;

  /// The channel open beside the list (tablet), if any.
  final String? selected;

  @override
  State<ChannelList> createState() => _ChannelListState();
}

class _ChannelListState extends State<ChannelList> {
  final _search = TextEditingController();
  bool _ticketsAll = false;
  bool _archivedAll = false;

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  Widget _row(ChannelView c) {
    final selected = c.conversationId == widget.selected;
    final ticket = isTicketChannel(c);
    final badges = !selected;
    return Semantics(
      label: channelSemantics(c),
      button: true,
      selected: selected,
      excludeSemantics: true,
      child: ListTile(
        key: ValueKey('channel-${c.conversationId}'),
        minTileHeight: 44,
        selected: selected,
        selectedTileColor: Tokens.fill,
        title: Text(
          channelLabel(c),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(fontWeight: c.unread > 0 ? FontWeight.w700 : FontWeight.w400, color: c.unread > 0 ? Tokens.text : Tokens.text2),
        ),
        subtitle: !ticket && c.lastActivityMs == null
            ? null
            : Row(children: [
                if (ticket && c.ticketColumn != null) ...[ChannelStatusPill(column: c.ticketColumn!), const SizedBox(width: 6)],
                if (c.lastActivityMs != null) Flexible(child: Text(Fmt.ago(c.lastActivityMs!), maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.label2, fontSize: 12))),
                if (c.archived) Text(' · read-only', style: TextStyle(color: Tokens.label2, fontSize: 12)),
              ]),
        trailing: !badges || (c.mentions == 0 && c.unread == 0)
            ? null
            : c.mentions > 0
                // A mention is an @, never a colour alone; it replaces the count (§1).
                ? Container(
                    key: ValueKey('channel-mentions-${c.conversationId}'),
                    padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
                    decoration: BoxDecoration(color: Tokens.accent, borderRadius: BorderRadius.circular(999)),
                    child: Text('@ ${c.mentions}', style: TextStyle(color: Tokens.accentInk, fontSize: 12, fontWeight: FontWeight.w600)),
                  )
                : Container(
                    key: ValueKey('channel-unread-${c.conversationId}'),
                    padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
                    decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(999)),
                    child: Text('${c.unread}', style: TextStyle(color: Tokens.label2, fontSize: 12)),
                  ),
        onTap: () => widget.onOpen(c),
      ),
    );
  }

  Widget _more(String key, int hidden, VoidCallback onTap) => ListTile(
        key: ValueKey(key),
        minTileHeight: 44,
        title: Text('Show more ($hidden)', style: TextStyle(color: Tokens.accentText, fontSize: 14)),
        onTap: onTap,
      );

  static const _eyebrow = EdgeInsets.fromLTRB(16, 14, 16, 4);

  @override
  Widget build(BuildContext context) {
    final state = context.watch<ChannelsBloc>().state;
    final searching = state.query.isNotEmpty && _search.text.trim().isNotEmpty;
    final List<Widget> rows;
    if (searching) {
      final found = state.found;
      rows = [
        if (found == null) const Padding(padding: EdgeInsets.all(24), child: Center(child: CircularProgressIndicator())),
        if (state.searchError != null)
          Padding(padding: const EdgeInsets.all(16), child: Text('The search failed: ${state.searchError}', key: const ValueKey('channel-search-error'), style: TextStyle(color: Tokens.negative, fontSize: 13))),
        if (found != null && found.isEmpty && state.searchError == null)
          Padding(padding: const EdgeInsets.all(16), child: Text('No channel matches “${state.query}”.', style: TextStyle(color: Tokens.label2, fontSize: 14))),
        for (final c in found ?? const <ChannelView>[]) _row(c),
      ];
    } else {
      final sections = channelSections(state.channels);
      final tickets = sectionRows(sections.tickets, all: _ticketsAll);
      final archived = sectionRows(sections.archived, all: _archivedAll);
      rows = [
        if (state.error != null)
          Padding(padding: const EdgeInsets.fromLTRB(16, 8, 16, 0), child: Text('The channels could not be read again: ${state.error}', key: const ValueKey('channels-error'), style: TextStyle(color: Tokens.negative, fontSize: 13))),
        const Eyebrow('Channels', padding: _eyebrow),
        for (final c in sections.channels) _row(c),
        if (sections.tickets.isNotEmpty) ...[
          const Eyebrow('Tickets', padding: _eyebrow),
          for (final c in tickets.shown) _row(c),
          if (tickets.hidden > 0) _more('channels-tickets-more', tickets.hidden, () => setState(() => _ticketsAll = true)),
        ],
        if (sections.archived.isNotEmpty)
          ExpansionTile(
            key: const ValueKey('channels-archived'),
            title: Eyebrow('Archived (${sections.archived.length})', padding: EdgeInsets.zero),
            children: [
              for (final c in archived.shown) _row(c),
              if (archived.hidden > 0) _more('channels-archived-more', archived.hidden, () => setState(() => _archivedAll = true)),
            ],
          ),
      ];
    }
    return ListView(
      key: const ValueKey('channel-list'),
      padding: EdgeInsets.zero,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
          child: CupertinoSearchTextField(
            key: const ValueKey('channel-search'),
            controller: _search,
            placeholder: 'Search channels or #42',
            onChanged: (q) {
              setState(() {});
              context.read<ChannelsBloc>().add(ChannelsSearched(q));
            },
          ),
        ),
        ...rows,
      ],
    );
  }
}

/// A ticket's status: its column's name in the small pill of the board (§6).
class ChannelStatusPill extends StatelessWidget {
  const ChannelStatusPill({super.key, required this.column});

  final ColumnKind column;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 1),
        decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(999)),
        child: Text(columnName(column), style: TextStyle(fontSize: 11.5, color: Tokens.label2)),
      );
}
