import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/refresh.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'my_tickets_bloc.dart';

/// What My tickets says when a tab has nothing (ui §5.3).
String myTabEmptyText(MyTab tab) => switch (tab) {
      MyTab.responsible => "You're not responsible for any open ticket. You'd follow up and review; employees do the work.",
      MyTab.created => "You haven't created a ticket yet.",
      MyTab.following => "You don't follow any open ticket. You follow one by creating it, becoming responsible, being @mentioned or pressing Follow.",
      MyTab.waiting => 'Nothing waits for you.',
    };

/// My tickets under the Board's segmented control (US-A4): four lists of the
/// signed-in board member's tickets, on the same cards as the Board. It reads
/// its own `ticket.my_tickets` while it shows and makes the tabs from that one answer.
class MyTicketsPane extends StatefulWidget {
  const MyTicketsPane({super.key, required this.me, required this.needsMe, required this.cardFor, required this.online});

  final String? me;

  /// The ◆ of a card for [me] (B6): the same function as the Board's, so Waiting on me and the cards agree.
  final bool Function(TicketView ticket) needsMe;

  /// The card for a ticket, with the Board's columns around it; null while the Board is not read yet.
  final Widget? Function(TicketView ticket) cardFor;
  final bool online;

  @override
  State<MyTicketsPane> createState() => _MyTicketsPaneState();
}

class _MyTicketsPaneState extends State<MyTicketsPane> {
  late final MyTicketsBloc _bloc = MyTicketsBloc(context.read<AureliusRepository>());

  /// The chosen tab; until one is chosen, Waiting on me when something waits, else Responsible (§5.2).
  MyTab? _tab;
  bool? _shown;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final shown = TickerMode.valuesOf(context).enabled;
    if (shown != _shown) {
      _shown = shown;
      _bloc.add(MyTicketsShown(shown));
    }
  }

  @override
  void dispose() {
    _bloc.close();
    super.dispose();
  }

  Future<void> _pickProject(BuildContext context, MyTicketsState state, CompanyState company) async {
    final picked = await showModalBottomSheet<(String?,)>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      builder: (sheet) => SafeArea(
        child: ListView(shrinkWrap: true, children: [
          Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Project')),
          ListTile(key: const ValueKey('my-project-all'), title: const Text('All projects'), trailing: state.projectId == null ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (null,))),
          for (final p in company.visibleProjects)
            ListTile(key: ValueKey('my-project-${p.id}'), title: Text(p.name), trailing: state.projectId == p.id ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (p.id,))),
        ]),
      ),
    );
    if (picked != null) _bloc.add(MyTicketsProjectChosen(picked.$1));
  }

  @override
  Widget build(BuildContext context) => BlocProvider.value(
        value: _bloc,
        child: BlocBuilder<MyTicketsBloc, MyTicketsState>(builder: (context, state) {
          final company = context.watch<CompanyBloc>().state;
          List<TicketView> of(MyTab tab) => state.inTab(tab, me: widget.me, needsMe: widget.needsMe);
          final tab = _tab ?? (state.mine == null ? MyTab.responsible : of(MyTab.waiting).isNotEmpty ? MyTab.waiting : MyTab.responsible);
          final shown = of(tab);
          final project = company.project(state.projectId ?? '');
          final Widget body;
          if (state.mine == null && state.error != null) {
            body = EmptyState(
              'Could not load your tickets.',
              detail: state.error,
              action: OutlinedButton(key: const ValueKey('my-tickets-retry'), onPressed: () => _bloc.add(const MyTicketsRefreshed()), child: const Text('Try again')),
            );
          } else if (state.mine == null) {
            body = const _Skeleton();
          } else if (shown.isEmpty) {
            body = state.projectId != null
                ? EmptyState(
                    'No tickets in ${project?.name ?? 'this project'}.',
                    action: TextButton(key: const ValueKey('my-tickets-clear'), onPressed: () => _bloc.add(const MyTicketsProjectChosen(null)), child: const Text('Clear filter')),
                  )
                : EmptyState(
                    myTabEmptyText(tab),
                    action: tab == MyTab.created ? FilledButton(key: const ValueKey('my-tickets-new'), onPressed: () => context.push('/board/new'), child: const Text('New ticket')) : null,
                  );
          } else if (shown.every((t) => widget.cardFor(t) == null)) {
            // The Board's columns are not read yet: they are what a card needs around it.
            body = const _Skeleton();
          } else {
            body = PullToRefresh(
              scrollable: true,
              onRefresh: () => untilReloaded(_bloc, () => _bloc.add(const MyTicketsRefreshed()), busy: (s) => s.loading),
              child: ListView(
                key: ValueKey('my-tickets-${tab.name}'),
                padding: EdgeInsets.fromLTRB(16, 8, 16, listBottom(context)),
                children: [
                  for (final t in shown)
                    if (widget.cardFor(t) case final card?) Padding(padding: const EdgeInsets.only(bottom: 10), child: card),
                ],
              ),
            );
          }
          return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            SizedBox(
              height: 52,
              child: SingleChildScrollView(
                key: const ValueKey('my-tickets-chips'),
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                child: Row(children: [
                  for (final (i, t) in MyTab.values.indexed)
                    Padding(
                      padding: const EdgeInsets.only(right: 8),
                      child: Semantics(
                        selected: t == tab,
                        label: '${t.label}, tab ${i + 1} of ${MyTab.values.length}, ${of(t).length} ticket${of(t).length == 1 ? '' : 's'}',
                        excludeSemantics: true,
                        child: ChoiceChip(
                          key: ValueKey('my-tab-${t.name}'),
                          selected: t == tab,
                          showCheckmark: false,
                          selectedColor: Tokens.accentTint,
                          label: Row(mainAxisSize: MainAxisSize.min, children: [
                            if (t == MyTab.waiting && of(t).isNotEmpty) ...[StatusDot(color: Tokens.accent, size: 7), const SizedBox(width: 6)],
                            Text('${MediaQuery.textScalerOf(context).scale(14) > 20 ? t.short : t.label} ${of(t).length}'),
                          ]),
                          onSelected: (_) => setState(() => _tab = t),
                        ),
                      ),
                    ),
                ]),
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 4),
              child: Row(children: [
                Flexible(
                  child: FilterChip(
                    key: const ValueKey('my-filter-project'),
                    label: Row(mainAxisSize: MainAxisSize.min, children: [Flexible(child: Text(project?.name ?? 'All projects', overflow: TextOverflow.ellipsis)), const SizedBox(width: 2), const Icon(Icons.expand_more, size: 16)]),
                    selected: project != null,
                    showCheckmark: false,
                    onSelected: (_) => _pickProject(context, state, company),
                  ),
                ),
                const SizedBox(width: 8),
                const Text('Show Done'),
                const SizedBox(width: 8),
                Switch.adaptive(key: const ValueKey('my-show-done'), value: state.includeDone, onChanged: (on) => _bloc.add(MyTicketsDoneShown(on))),
              ]),
            ),
            if (!widget.online) Padding(padding: const EdgeInsets.fromLTRB(20, 0, 20, 4), child: Text('Offline · reconnecting', style: TextStyle(color: Tokens.label2))),
            Expanded(child: Opacity(opacity: widget.online ? 1 : 0.6, child: body)),
          ]);
        }),
      );
}

/// While the first `ticket.my_tickets` is on its way: skeleton cards (no animation).
class _Skeleton extends StatelessWidget {
  const _Skeleton();

  @override
  Widget build(BuildContext context) => ListView(
        key: const ValueKey('my-tickets-skeleton'),
        padding: const EdgeInsets.all(16),
        children: [
          for (var i = 0; i < 4; i++) Container(height: 96, margin: const EdgeInsets.only(bottom: 10), decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(18))),
        ],
      );
}
