import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/refresh.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/links.dart';
import '../../data/quick_task.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import 'board_bloc.dart';
import 'ticket_actions.dart';
import 'ticket_card.dart';

/// From this width the columns stand side by side and cards can be dragged (§4.3).
const boardWideWidth = 700.0;

/// Tells the [BoardBloc] whether its tab is on screen: the shell turns the
/// tickers of hidden tabs (and of pages under another one) off.
class BoardShownWatcher extends StatefulWidget {
  const BoardShownWatcher({super.key, required this.child});

  final Widget child;

  @override
  State<BoardShownWatcher> createState() => _BoardShownWatcherState();
}

class _BoardShownWatcherState extends State<BoardShownWatcher> {
  bool? _shown;
  BoardBloc? _bloc;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _bloc = context.read<BoardBloc>();
    final shown = TickerMode.valuesOf(context).enabled;
    if (shown != _shown) {
      _shown = shown;
      _bloc!.add(BoardShown(shown));
    }
  }

  /// A project's Board tab is taken out of the tree when another tab shows,
  /// while its bloc lives on with the project page.
  @override
  void dispose() {
    final bloc = _bloc;
    if (bloc != null && !bloc.isClosed) bloc.add(const BoardShown(false));
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// The Board (§4, MU1-05): on a phone one column at a time (chips and a
/// swipe between them), from [boardWideWidth] all side by side with drag and
/// drop. Moves without dragging: the card's button, a swipe, Move to….
///
/// It is a project's Board tab: the project page has the app bar (and its +
/// is New ticket), and the project is fixed, so there is no project filter.
class BoardPage extends StatefulWidget {
  const BoardPage({super.key});

  @override
  State<BoardPage> createState() => _BoardPageState();
}

/// Whether nobody chooses [ticket]'s owner in Todo: no dispatcher at all, or it gave up (B5).
bool needsOwnerOf(TicketView ticket, CompanyState company, BoardState board) =>
    ticket.column == ColumnKind.todo &&
    ticket.ownerEmployeeId == null &&
    (board.gaveUp.contains(ticket.id) || (!company.loading && effectiveDispatcher(company.project(ticket.projectId), company.company?.dispatcherEmployeeId, company.employees) == null));

/// [ticket]'s state with what the company and board know.
TicketState stateOf(TicketView ticket, CompanyState company, BoardState board, {String? me}) => ticketState(ticket,
    root: company.tasks.where((t) => t.id == ticket.rootTaskId).firstOrNull, needsOwner: needsOwnerOf(ticket, company, board), ownerUnknown: board.ownerUnknown.contains(ticket.id), me: me);

/// Whether [ticket] counts as [me]'s work (B6): the ◆ on its card, the filter and the count.
bool needsMeOf(TicketView ticket, CompanyState company, BoardState board, {String? me}) => needsMe(ticket, stateOf(ticket, company, board, me: me), me: me);

class _BoardPageState extends State<BoardPage> {
  PageController? _pages;
  bool _olderDone = false;
  String? _me;
  bool _myLoginExpired = false;
  TicketView? _dragging;

  @override
  void dispose() {
    _pages?.dispose();
    super.dispose();
  }

  /// Where the phone opens: the last column viewed, else the first with
  /// something for you (Review, Plan review, a Needs you in Todo/Doing), else Doing (§4.1).
  ColumnKind _opening(BoardState board, CompanyState company) {
    if (board.column != null) return board.column!;
    for (final kind in [ColumnKind.review, ColumnKind.planReview, ColumnKind.todo, ColumnKind.doing]) {
      final column = board.columnOf(kind);
      if (column != null && column.tickets.any((t) => needsMeOf(t, company, board, me: _me))) return kind;
    }
    return ColumnKind.doing;
  }

  void _show(BuildContext context, List<BoardColumnCards> columns, ColumnKind kind) {
    final index = columns.indexWhere((c) => c.column.kind == kind);
    if (index < 0) return;
    context.read<BoardBloc>().add(BoardColumnShown(kind));
    _pages?.animateToPage(index, duration: const Duration(milliseconds: 220), curve: Curves.easeOut);
  }

  Future<void> _pickOwner(BuildContext context, BoardState board, CompanyState company) async {
    final employees = company.activeEmployees.toList()..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    final picked = await showModalBottomSheet<(String?,)>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(shrinkWrap: true, children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Owner')),
            ListTile(title: const Text('Anyone'), trailing: board.owner == null ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (null,))),
            ListTile(title: const Text('No owner yet'), trailing: board.owner == BoardState.noOwner ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (BoardState.noOwner,))),
            for (final e in employees) ListTile(title: Text(e.name), trailing: board.owner == e.id ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (e.id,))),
          ]),
        ),
      ),
    );
    if (picked != null && context.mounted) context.read<BoardBloc>().add(BoardOwnerChosen(picked.$1));
  }

  /// "Created by: Anyone / Me / a board member" (ui §3, R3 m6). The board
  /// members come from `user.list`; without it, the creators on the board.
  Future<void> _pickCreator(BuildContext context, BoardState board) async {
    final repository = context.read<AureliusRepository>();
    final bloc = context.read<BoardBloc>();
    final me = _me;
    var members = <(String, String)>[];
    try {
      members = [for (final u in await repository.boardMembers()) if (u.disabledAtMs == null || board.createdBy == u.id) (u.id, u.username)];
    } on Object {
      members = {for (final t in board.board?.columns.expand((c) => c.tickets) ?? const <TicketView>[]) t.createdByUser: t.createdByUsername}.entries.map((e) => (e.key, e.value)).toList();
    }
    members = members.where((m) => m.$1 != me).toList()..sort((a, b) => a.$2.toLowerCase().compareTo(b.$2.toLowerCase()));
    _creatorNames.addAll({for (final (id, name) in members) id: name});
    if (!context.mounted) return;
    Widget tick(String? id) => board.createdBy == id ? Icon(Icons.check, color: Tokens.accentText) : const SizedBox.shrink();
    final picked = await showModalBottomSheet<(String?,)>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(shrinkWrap: true, children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Created by')),
            ListTile(key: const ValueKey('board-creator-anyone'), title: const Text('Anyone'), trailing: tick(null), onTap: () => Navigator.pop(sheet, (null,))),
            if (me != null) ListTile(key: const ValueKey('board-creator-me'), title: const Text('Me'), trailing: tick(me), onTap: () => Navigator.pop(sheet, (me,))),
            for (final (id, name) in members) ListTile(key: ValueKey('board-creator-$id'), title: Text(name), trailing: tick(id), onTap: () => Navigator.pop(sheet, (id,))),
          ]),
        ),
      ),
    );
    if (picked != null) bloc.add(BoardCreatorChosen(picked.$1));
  }

  /// The creator filter's name: Me, the board member, or null for anyone.
  String? _creatorName(BoardState board) {
    final id = board.createdBy;
    if (id == null) return null;
    if (id == _me) return 'Me';
    return board.board?.columns.expand((c) => c.tickets).where((t) => t.createdByUser == id).firstOrNull?.createdByUsername ?? _creatorNames[id] ?? 'Board member';
  }

  /// Board members named in the creator picker, for a filter without tickets.
  final _creatorNames = <String, String>{};

  /// "Responsible: Anyone / Me / No one / a board member" (P-77); below them the
  /// disabled ones who still have tickets here, as those stay on their cards.
  Future<void> _pickResponsible(BuildContext context, BoardState board) async {
    final repository = context.read<AureliusRepository>();
    final bloc = context.read<BoardBloc>();
    final me = _me;
    final onBoard = {for (final t in board.board?.columns.expand((c) => c.tickets) ?? const <TicketView>[]) if (t.responsibleUserId != null) t.responsibleUserId!: t.responsibleUsername ?? 'Board member'};
    var active = <(String, String)>[];
    var disabled = <(String, String)>[];
    try {
      final users = await repository.boardMembers();
      active = [for (final u in users) if (u.disabledAtMs == null && u.id != me) (u.id, u.username)];
      disabled = [for (final u in users) if (u.disabledAtMs != null && u.id != me && onBoard.containsKey(u.id)) (u.id, u.username)];
    } on Object {
      active = [for (final e in onBoard.entries) if (e.key != me) (e.key, e.value)];
    }
    int byName((String, String) a, (String, String) b) => a.$2.toLowerCase().compareTo(b.$2.toLowerCase());
    active.sort(byName);
    disabled.sort(byName);
    _responsibleNames.addAll({for (final (id, name) in [...active, ...disabled]) id: name});
    if (!context.mounted) return;
    Widget? tick(String? id) => board.responsible == id ? Icon(Icons.check, color: Tokens.accentText) : null;
    final picked = await showModalBottomSheet<(String?,)>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(shrinkWrap: true, children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Responsible')),
            ListTile(key: const ValueKey('board-responsible-anyone'), title: const Text('Anyone'), trailing: tick(null), onTap: () => Navigator.pop(sheet, (null,))),
            if (me != null) ListTile(key: const ValueKey('board-responsible-me'), title: const Text('Me'), trailing: tick(me), onTap: () => Navigator.pop(sheet, (me,))),
            ListTile(key: const ValueKey('board-responsible-no-one'), title: const Text('No one'), trailing: tick(BoardState.noResponsible), onTap: () => Navigator.pop(sheet, (BoardState.noResponsible,))),
            for (final (id, name) in active) ListTile(key: ValueKey('board-responsible-$id'), title: Text(name), trailing: tick(id), onTap: () => Navigator.pop(sheet, (id,))),
            if (disabled.isNotEmpty) Padding(padding: const EdgeInsets.fromLTRB(16, 12, 16, 4), child: Eyebrow('Disabled')),
            for (final (id, name) in disabled) ListTile(key: ValueKey('board-responsible-$id'), title: Text(name), trailing: tick(id), onTap: () => Navigator.pop(sheet, (id,))),
          ]),
        ),
      ),
    );
    if (picked != null) bloc.add(BoardResponsibleChosen(picked.$1));
  }

  /// The responsible filter's name: Me, No one, the board member, or null for anyone.
  String? _responsibleName(BoardState board) {
    final id = board.responsible;
    if (id == null) return null;
    if (id == BoardState.noResponsible) return 'No one';
    if (id == _me) return 'Me';
    return board.board?.columns.expand((c) => c.tickets).where((t) => t.responsibleUserId == id).firstOrNull?.responsibleUsername ?? _responsibleNames[id] ?? 'Board member';
  }

  /// Board members named in the responsible picker, for a filter without tickets.
  final _responsibleNames = <String, String>{};

  void _newTicket(BuildContext context, BoardState board) => context.push(board.projectId == null ? '/board/new' : '/board/new?project=${board.projectId}');

  @override
  Widget build(BuildContext context) {
    final board = context.watch<BoardBloc>().state;
    final company = context.watch<CompanyBloc>().state;
    final link = context.watch<ConnectionBloc>().state;
    final online = link.isOnline;
    _me = link.user?.id;
    _myLoginExpired = link.claudeLogin?.status == ClaudeLoginStatus.expired;
    final wide = MediaQuery.sizeOf(context).width >= boardWideWidth;
    final responsibleOn = link.has(ticketResponsibleFeature);
    final quickOn = quickTasksOn(link);
    final columns = !responsibleOn || !board.needsMeOnly ? board.columns : [for (final c in board.columns) BoardColumnCards(column: c.column, tickets: [for (final t in c.tickets) if (needsMeOf(t, company, board, me: _me)) t])];
    final tickets = [for (final c in columns) ...c.tickets];
    final needYou = tickets.where((t) => needsMeOf(t, company, board, me: _me)).length;
    final owner = board.owner == null ? null : board.owner == BoardState.noOwner ? 'No owner yet' : company.employee(board.owner!)?.name;
    final creator = _creatorName(board);
    final responsible = responsibleOn ? _responsibleName(board) : null;

    final Widget content;
    if (board.board == null && board.error != null) {
      content = board.unsupported
          ? const EmptyState('This server does not have the Board yet.', detail: 'Update the server.')
          : EmptyState(
              'Could not load the Board.',
              detail: board.error,
              action: OutlinedButton(key: const ValueKey('board-retry'), onPressed: () => context.read<BoardBloc>().add(const BoardRefreshed()), child: const Text('Try again')),
            );
    } else if (board.board == null) {
      content = const _Skeleton();
    } else {
      final filteredEmpty = tickets.isEmpty && (board.owner != null || board.createdBy != null || responsible != null || (responsibleOn && board.needsMeOnly) || (quickOn && board.quickOnly));
      final body = wide ? _wide(context, board, company, columns, online) : _phone(context, board, company, columns, online);
      content = Column(children: [
        if (filteredEmpty)
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 4, 12, 0),
            child: Row(children: [
              Expanded(child: Text(responsible != null && responsible != 'No one' && responsible != 'Me' ? 'No tickets for $responsible.' : quickOn && board.quickOnly ? 'No quick tasks.' : 'No tickets in ${owner ?? (creator == null ? null : 'Created by: $creator') ?? 'this filter'}.', style: TextStyle(color: Tokens.label2))),
              TextButton(
                onPressed: () {
                  final bloc = context.read<BoardBloc>();
                  bloc.add(const BoardOwnerChosen(null));
                  bloc.add(const BoardCreatorChosen(null));
                  bloc.add(const BoardResponsibleChosen(null));
                  bloc.add(const BoardNeedsMeOnly(false));
                  bloc.add(const BoardQuickOnly(false));
                },
                child: const Text('Clear filter'),
              ),
            ]),
          ),
        Expanded(child: Opacity(opacity: online ? 1 : 0.6, child: body)),
      ]);
    }

    final page = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (board.board != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 4, 20, 0),
            child: Text(
              '${tickets.length} ticket${tickets.length == 1 ? '' : 's'}${needYou == 0 ? '' : ' · $needYou need${needYou == 1 ? 's' : ''} you'}${online ? '' : ' · Offline · reconnecting'}',
              key: const ValueKey('board-count'),
              style: TextStyle(color: Tokens.label2),
            ),
          ),
        SizedBox(
          height: 52,
          // Every filter built, as the column chips: a lazy list would leave some out for screen readers.
          child: SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            child: Row(children: [
              if (responsibleOn)
                Padding(
                  padding: const EdgeInsets.only(right: 8),
                  child: FilterChip(
                    key: const ValueKey('board-filter-needs-you'),
                    label: const Text('◆ Needs you'),
                    selected: board.needsMeOnly,
                    showCheckmark: false,
                    onSelected: (on) => context.read<BoardBloc>().add(BoardNeedsMeOnly(on)),
                  ),
                ),
              if (quickOn)
                Padding(
                  padding: const EdgeInsets.only(right: 8),
                  child: FilterChip(
                    key: const ValueKey('board-filter-quick'),
                    avatar: const Icon(CupertinoIcons.bolt, size: 16),
                    label: const Text(quickFilterName),
                    selected: board.quickOnly,
                    showCheckmark: false,
                    onSelected: (on) => context.read<BoardBloc>().add(BoardQuickOnly(on)),
                  ),
                ),
              if (responsibleOn) _FilterPill(key: const ValueKey('board-filter-responsible'), label: 'Responsible: ${responsible ?? 'Anyone'}', active: responsible != null, onTap: () => _pickResponsible(context, board)),
              _FilterPill(key: const ValueKey('board-filter-owner'), label: 'Owner: ${owner ?? 'Anyone'}', active: owner != null, onTap: () => _pickOwner(context, board, company)),
              _FilterPill(key: const ValueKey('board-filter-creator'), label: 'Created by: ${creator ?? 'Anyone'}', active: creator != null, onTap: () => _pickCreator(context, board)),
            ]),
          ),
        ),
        Expanded(child: content),
      ],
    );

    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.keyN, meta: true): () => _newTicket(context, board),
        const SingleActivator(LogicalKeyboardKey.keyN, control: true): () => _newTicket(context, board),
      },
      child: page,
    );
  }

  // ── Phone: one column at a time ────────────────────────────────────────

  Widget _phone(BuildContext context, BoardState board, CompanyState company, List<BoardColumnCards> columns, bool online) {
    final opening = _opening(board, company);
    final index = columns.indexWhere((c) => c.column.kind == opening).clamp(0, columns.length - 1);
    _pages ??= PageController(initialPage: index);
    return Column(children: [
      // Every column's chip: all built (a lazy list would leave some out for screen readers).
      SingleChildScrollView(
        key: const ValueKey('board-chips'),
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
        child: Row(children: [
          for (final (i, c) in columns.indexed) _ColumnChip(column: c, index: i, total: columns.length, selected: i == index, needsYou: c.column.kind == ColumnKind.review && c.tickets.any((t) => needsMeOf(t, company, board, me: _me)), onTap: () => _show(context, columns, c.column.kind)),
        ]),
      ),
      Expanded(
        child: PageView(
          key: const ValueKey('board-pages'),
          controller: _pages,
          onPageChanged: (i) => context.read<BoardBloc>().add(BoardColumnShown(columns[i].column.kind)),
          children: [for (final c in columns) _columnList(context, board, company, columns, c, online, wide: false)],
        ),
      ),
    ]);
  }

  // ── Tablet: side by side, drag and drop ────────────────────────────────

  Widget _wide(BuildContext context, BoardState board, CompanyState company, List<BoardColumnCards> columns, bool online) => ListView(
        key: const ValueKey('board-wide'),
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.fromLTRB(16, 4, 16, 16),
        children: [
          for (final c in columns)
            Padding(
              padding: const EdgeInsets.only(right: 12),
              child: c.column.kind == ColumnKind.planReview && c.tickets.isEmpty && _dragging == null
                  ? _CollapsedColumn(column: c)
                  : SizedBox(width: 280, child: _wideColumn(context, board, company, columns, c, online)),
            ),
        ],
      );

  Widget _wideColumn(BuildContext context, BoardState board, CompanyState company, List<BoardColumnCards> columns, BoardColumnCards c, bool online) {
    final dragging = _dragging;
    final rule = dragging == null ? null : moveRule(dragging.column, c.column.kind);
    final refused = rule is MoveRefused ? rule.reason : null;
    final allowed = rule is MoveAllowed;
    return DragTarget<TicketView>(
      onWillAcceptWithDetails: (d) => moveRule(d.data.column, c.column.kind) is MoveAllowed,
      onAcceptWithDetails: (d) => moveTicket(context, d.data, c.column),
      builder: (context, candidates, _) => AnimatedOpacity(
        duration: const Duration(milliseconds: 120),
        opacity: refused != null ? 0.5 : 1,
        child: Container(
          decoration: BoxDecoration(
            color: allowed && dragging != null ? Tokens.accentTint : Tokens.cell.withValues(alpha: 0.4),
            borderRadius: BorderRadius.circular(18),
            border: allowed && dragging != null ? Border.all(color: Tokens.accent.withValues(alpha: 0.5)) : null,
          ),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            _ColumnHeader(column: c, note: refused, needsYou: c.column.kind == ColumnKind.review && c.tickets.any((t) => needsMeOf(t, company, board, me: _me))),
            Expanded(child: _columnList(context, board, company, columns, c, online, wide: true)),
          ]),
        ),
      ),
    );
  }

  // ── One column's cards ─────────────────────────────────────────────────

  Widget _columnList(BuildContext context, BoardState board, CompanyState company, List<BoardColumnCards> columns, BoardColumnCards c, bool online, {required bool wide}) {
    final kind = c.column.kind;
    final emptyBoard = board.tickets.isEmpty && board.projectId == null && board.owner == null;
    var cards = c.tickets;
    var older = 0;
    if (kind == ColumnKind.done) {
      cards = [...cards]..sort((a, b) => (b.doneAtMs ?? 0).compareTo(a.doneAtMs ?? 0));
      if (!_olderDone && cards.length > 20) {
        older = cards.length - 20;
        cards = cards.take(20).toList();
      }
    }
    final allColumns = [for (final x in columns) x.column];
    Widget card(TicketView t) {
      final ctx = CardContext(columns: allColumns, column: c, needsOwner: needsOwnerOf(t, company, board), ownerUnknown: board.ownerUnknown.contains(t.id), showProject: board.projectId == null, online: online, wide: wide, me: _me, myLoginExpired: _myLoginExpired, responsible: context.read<ConnectionBloc>().state.has(ticketResponsibleFeature), follow: context.read<ConnectionBloc>().state.has(ticketFollowFeature), links: context.read<ConnectionBloc>().state.has(messageLocateFeature), quick: quickTasksOn(context.read<ConnectionBloc>().state));
      final face = TicketCard(key: ValueKey('ticket-${t.id}'), ticket: t, board: ctx);
      if (!wide || !online || kind == ColumnKind.done) return face;
      // Drop on a card: it lands above it.
      return DragTarget<TicketView>(
        onWillAcceptWithDetails: (d) => d.data.id != t.id && moveRule(d.data.column, kind) is MoveAllowed,
        onAcceptWithDetails: (d) => moveTicket(context, d.data, c.column, before: t.id),
        builder: (context, candidates, _) => Column(children: [
          if (candidates.isNotEmpty) Container(height: 3, margin: const EdgeInsets.only(bottom: 4), color: Tokens.accent),
          LongPressDraggable<TicketView>(
            data: t,
            onDragStarted: () {
              HapticFeedback.mediumImpact();
              setState(() => _dragging = t);
            },
            onDragEnd: (_) => setState(() => _dragging = null),
            feedback: Material(color: Colors.transparent, elevation: 8, child: SizedBox(width: 264, child: face)),
            childWhenDragging: Opacity(opacity: 0.3, child: face),
            child: face,
          ),
        ]),
      );
    }

    final children = <Widget>[
      if (cards.isEmpty)
        emptyBoard && kind == ColumnKind.backlog
            ? Padding(
                padding: const EdgeInsets.only(top: 24),
                child: EmptyState(
                  'No tickets yet',
                  detail: 'New tickets land in Backlog. Nothing starts until you press Start.',
                  action: FilledButton(key: const ValueKey('board-empty-new'), onPressed: () => _newTicket(context, board), child: const Text('New ticket')),
                ),
              )
            : Padding(padding: const EdgeInsets.fromLTRB(8, 16, 8, 8), child: Text(kind.emptyText, key: ValueKey('board-empty-${kind.wire}'), style: TextStyle(color: Tokens.label2))),
      for (final t in cards) Padding(padding: const EdgeInsets.only(bottom: 10), child: card(t)),
      if (older > 0) TextButton(onPressed: () => setState(() => _olderDone = true), child: Text('Show older ($older)')),
      if (kind == ColumnKind.backlog && !(emptyBoard && cards.isEmpty))
        TextButton.icon(key: const ValueKey('board-add-ticket'), onPressed: () => _newTicket(context, board), icon: const Icon(CupertinoIcons.add), label: const Text('Add ticket')),
    ];
    final bloc = context.read<BoardBloc>();
    return PullToRefresh(
      scrollable: true,
      onRefresh: () => untilReloaded(bloc, () => bloc.add(const BoardRefreshed()), busy: (s) => s.loading),
      child: ListView(
        key: ValueKey('board-column-${kind.wire}'),
        padding: EdgeInsets.fromLTRB(wide ? 8 : 16, 8, wide ? 8 : 16, wide ? 16 : listBottom(context)),
        children: children,
      ),
    );
  }
}

class _FilterPill extends StatelessWidget {
  const _FilterPill({super.key, required this.label, required this.active, required this.onTap});

  final String label;
  final bool active;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(right: 8),
        child: FilterChip(
          label: Row(mainAxisSize: MainAxisSize.min, children: [Text(label), const SizedBox(width: 2), const Icon(Icons.expand_more, size: 16)]),
          selected: active,
          showCheckmark: false,
          onSelected: (_) => onTap(),
        ),
      );
}

/// A column chip on the phone: name and count; Review gets a green dot while
/// something waits for you (B6); an empty Plan review is narrow and dimmed (D-04).
class _ColumnChip extends StatelessWidget {
  const _ColumnChip({required this.column, required this.index, required this.total, required this.selected, this.needsYou = false, required this.onTap});

  final BoardColumnCards column;
  final int index;
  final int total;
  final bool selected;
  final bool needsYou;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final count = column.tickets.length;
    final quiet = column.column.kind == ColumnKind.planReview && count == 0 && !selected;
    return Padding(
      padding: const EdgeInsets.only(right: 8),
      child: Semantics(
        selected: selected,
        label: '${column.column.name}, tab ${index + 1} of $total, $count ticket${count == 1 ? '' : 's'}',
        excludeSemantics: true,
        child: Opacity(
          opacity: quiet ? 0.55 : 1,
          child: ChoiceChip(
            key: ValueKey('board-chip-${column.column.kind.wire}'),
            selected: selected,
            showCheckmark: false,
            selectedColor: Tokens.accentTint,
            visualDensity: quiet ? VisualDensity.compact : null,
            label: Row(mainAxisSize: MainAxisSize.min, children: [
              if (needsYou) ...[StatusDot(color: Tokens.accent, size: 7), const SizedBox(width: 6)],
              Text('${column.column.name} $count', style: TextStyle(fontSize: quiet ? 12 : 14)),
            ]),
            onSelected: (_) => onTap(),
          ),
        ),
      ),
    );
  }
}

class _ColumnHeader extends StatelessWidget {
  const _ColumnHeader({required this.column, this.note, this.needsYou = false});

  final BoardColumnCards column;
  final String? note;
  final bool needsYou;

  @override
  Widget build(BuildContext context) => Semantics(
        header: true,
        label: '${column.column.name}, ${column.tickets.length} tickets',
        excludeSemantics: true,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 10, 12, 4),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              Expanded(child: Text(column.column.name, style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600))),
              if (needsYou) ...[const NeedsYouTag('◆ Needs you'), const SizedBox(width: 8)],
              Text('${column.tickets.length}', style: Tokens.monoSmall()),
            ]),
            if (note != null) Text(note!, style: TextStyle(fontSize: 12, color: Tokens.label2)),
          ]),
        ),
      );
}

/// An empty Plan review on a tablet: a 44 px strip with the name upright (D-04).
class _CollapsedColumn extends StatelessWidget {
  const _CollapsedColumn({required this.column});

  final BoardColumnCards column;

  @override
  Widget build(BuildContext context) => Tooltip(
        message: ColumnKind.planReview.emptyText,
        child: Container(
          key: const ValueKey('board-plan-review-strip'),
          width: 44,
          decoration: BoxDecoration(color: Tokens.cell.withValues(alpha: 0.4), borderRadius: BorderRadius.circular(18)),
          padding: const EdgeInsets.symmetric(vertical: 12),
          child: Column(children: [
            Text('0', style: Tokens.monoSmall()),
            const SizedBox(height: 8),
            RotatedBox(quarterTurns: 1, child: Text(column.column.name, style: TextStyle(color: Tokens.label2))),
          ]),
        ),
      );
}

/// While the first `board.get` is on its way: skeleton cards (no animation).
class _Skeleton extends StatelessWidget {
  const _Skeleton();

  @override
  Widget build(BuildContext context) => ListView(
        key: const ValueKey('board-skeleton'),
        padding: const EdgeInsets.all(16),
        children: [
          for (var i = 0; i < 3; i++)
            Container(height: 96, margin: const EdgeInsets.only(bottom: 10), decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(18))),
        ],
      );
}
