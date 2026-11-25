import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/discussion.dart';
import '../../data/commands.dart';
import '../../data/company.dart';
import '../../data/mode_memory.dart';
import '../../data/views.dart';
import '../chat/chat_bloc.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';
import '../connection/connection_banner.dart';
import '../tasks/give_work.dart';
import '../sessions/session_rules.dart' show workSessionsFeature;
import 'employee_run_sheet.dart';
import '../my_defaults/my_defaults_bloc.dart' show userRunDefaultsFeature;
import '../project/tasks_tab.dart';
import 'employee_widgets.dart';

/// An employee: give it work, where it sits (boss, team), where it
/// works now, its model and status, and its tasks across projects.
class EmployeePage extends StatelessWidget {
  const EmployeePage({super.key, required this.employeeId});

  final String employeeId;

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<CompanyBloc, CompanyState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<CompanyBloc>().add(const CompanyNoticeShown());
      },
      builder: (context, company) {
        final m = company.employee(employeeId);
        if (m == null) {
          return Scaffold(
            appBar: GlassAppBar(),
            body: company.loading ? const Center(child: CircularProgressIndicator()) : const EmptyState('This employee no longer exists'),
          );
        }
        final boss = company.employee(m.reportsTo ?? '');
        final reports = company.activeEmployees.where((x) => x.reportsTo == m.id).toList()..sort(byName);
        final projects = [for (final id in projectsOf(m.id, company.tasks)) ?company.project(id)];
        final tasks = company.tasks.where((t) => t.employeeId == m.id).toList()
          ..sort((a, b) {
            final open = (a.isOpen ? 0 : 1).compareTo(b.isOpen ? 0 : 1);
            return open != 0 ? open : b.updatedAtMs.compareTo(a.updatedAtMs);
          });
        final archived = m.status == LifecycleStatus.archived;
        return Scaffold(
          appBar: GlassAppBar(
            titleSpacing: 0,
            title: Row(
              children: [
                Monogram(m.monogram, size: 32, active: m.working),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(m.name, style: Tokens.title(size: 18)),
                      Text(
                        m.roleLine,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(fontSize: 12, color: Tokens.muted),
                      ),
                    ],
                  ),
                ),
              ],
            ),
            actions: [
              Padding(padding: const EdgeInsets.only(right: 4), child: Center(child: employeePresence(m))),
              if (!archived) GlassCapsule(children: [EmployeeMenu(employee: m)]),
            ],
          ),
          body: Column(
            children: [
              const ConnectionBanner(),
              if (m.agents.isNotEmpty) AgentsBar(employee: m),
              Expanded(
                child: ListView(
                  key: const ValueKey('employee-page'),
                  padding: EdgeInsets.only(bottom: listBottom(context)),
                  children: [
                    // Its persona and rules are in its instructions: the preview opens the whole text.
                    if ((m.instructions ?? '').isNotEmpty)
                      InkWell(
                        key: const ValueKey('employee-instructions'),
                        onTap: () => _showText(context, 'Instructions of ${m.name}', m.instructions!),
                        child: Padding(
                          padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(m.instructions!, maxLines: 3, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 14, color: Tokens.label2)),
                              const SizedBox(height: 2),
                              Text('Show all instructions', style: TextStyle(fontSize: 13, color: Tokens.accent)),
                            ],
                          ),
                        ),
                      ),
                    const SizedBox(height: 12),
                    if (!archived)
                      InsetGroup(
                        dividerIndent: 60,
                        children: [
                          GroupRow(
                            key: const ValueKey('employee-give-work'),
                            leading: const IconTile(CupertinoIcons.square_pencil),
                            title: 'Give ${m.name} work',
                            onTap: () => giveWork(context, employeeId: m.id),
                          ),
                        ],
                      ),
                    const GroupHeader('In the Company'),
                    InsetGroup(
                      children: [
                        GroupRow(
                          key: const ValueKey('employee-boss'),
                          title: 'Reports to',
                          value: boss?.name ?? 'Nobody · management team',
                          onTap: boss == null ? () => editEmployee(context, m) : () => context.push('/employee/${boss.id}'),
                        ),
                        if (reports.isNotEmpty)
                          GroupRow(
                            title: 'Team',
                            subtitle: reports.map((r) => r.name).join(', '),
                            onTap: () => _showPeople(context, 'Reporting to ${m.name}', reports),
                          ),
                        GroupRow(
                          title: 'Working in',
                          subtitle: projects.isEmpty ? 'No open tasks' : projects.map((p) => p.name).join(', '),
                          chevron: false,
                        ),
                      ],
                    ),
                    if (!archived) ...[
                      const GroupHeader('Setup'),
                      InsetGroup(
                        children: [
                          // Model, effort and mode are each board member's own there (Profile > My defaults).
                          if (!context.read<AureliusRepository>().linkState.has(userRunDefaultsFeature)) ...[
                            GroupRow(
                              key: const ValueKey('employee-model'),
                              title: 'Model and effort',
                              onTap: () => showEmployeeRunSheet(context, m),
                            ),
                            // Its own chat has no page; its group chat answers run there, with these.
                            GroupRow(key: const ValueKey('employee-mode'), title: 'Mode', value: m.agentMode.label, onTap: () => _pickMode(context, m)),
                          ],
                          SwitchListTile(
                            key: const ValueKey('employee-browser'),
                            title: const Text('Browser'),
                            subtitle: Text('Let ${m.name} use the browser.', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                            value: m.browserAllowed,
                            onChanged: (value) => context.read<CompanyBloc>().send(Commands.setBrowserAllowed(m.conversationId, value), retry: false),
                          ),
                          SwitchListTile(
                            key: const ValueKey('employee-screen'),
                            title: const Text('Screen'),
                            subtitle: Text('Screenshots of the PC without asking.', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                            value: m.screenAllowed,
                            onChanged: (value) => _setScreen(context, m, value),
                          ),
                          GroupRow(key: const ValueKey('employee-edit'), title: 'Edit', onTap: () => editEmployee(context, m)),
                          _Advanced(employee: m),
                        ],
                      ),
                    ],
                    if (context.read<AureliusRepository>().linkState.has(guidelinesFeature)) EmployeeGuidelines(key: ValueKey('employee-guidelines-${m.id}'), employee: m),
                    // Chats converted from before the company (one per old project thread): history.
                    if (m.conversations.where((c) => c.id != m.conversationId).toList() case final older when older.isNotEmpty) ...[
                      GroupHeader('Earlier Chats', count: older.length),
                      InsetGroup(
                        children: [
                          for (final c in older.take(10))
                            GroupRow(
                              key: ValueKey('employee-older-chat-${c.id}'),
                              title: c.title.isEmpty ? 'Conversation' : c.title,
                              subtitle: Fmt.ago(c.updatedAtMs),
                              value: 'Read-only',
                              onTap: () => context.push('/conversation/${c.id}'),
                            ),
                        ],
                      ),
                    ],
                    GroupHeader('Tasks', count: tasks.where((t) => t.isOpen).length, action: tasks.length > 8 ? 'See All' : null, onAction: () => context.push('/employee/${m.id}/tasks')),
                    InsetGroup(
                      children: [
                        if (tasks.isEmpty) GroupRow(title: '${m.name} has no tasks yet', chevron: false),
                        for (final t in tasks.take(8)) TaskRow(key: ValueKey('employee-task-${t.id}'), task: t),
                      ],
                    ),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  /// The mode of [m]'s own chat, where its group chat answers run.
  Future<void> _pickMode(BuildContext context, EmployeeSummary m) async {
    final company = context.read<CompanyBloc>();
    final repository = context.read<AureliusRepository>();
    final mode = await showModalBottomSheet<AgentMode>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final mode in agentModes)
              ListTile(
                key: ValueKey('employee-mode-${mode.wire}'),
                title: Text(mode.label),
                trailing: mode == m.agentMode ? Icon(Icons.check, color: Tokens.accent) : null,
                onTap: () => Navigator.pop(sheet, mode),
              ),
          ],
        ),
      ),
    );
    if (mode == null || mode == m.agentMode) return;
    company.send(Commands.setAgentMode(m.conversationId, mode), retry: false);
    // As in a conversation: its next task starts in this mode too (see ModeMemory).
    try {
      unawaited(ModeMemory.write(repository.host.hostId, m.id, mode.wire));
    } on Object {
      // No host (tests): not remembered.
    }
  }

  /// Turning the screen on asks first, as in a conversation: screenshots show anything that is open.
  Future<void> _setScreen(BuildContext context, EmployeeSummary m, bool value) async {
    final company = context.read<CompanyBloc>();
    if (value) {
      final ok = await showDialog<bool>(
        context: context,
        builder: (dialog) => AlertDialog(
          title: Text('Let ${m.name} see your screen?'),
          content: const Text(
            'It can then take screenshots of the PC screen or of one window without asking. '
            'A screenshot shows anything that is open. Off, it still asks you for each screenshot.',
          ),
          actions: [
            TextButton(onPressed: () => Navigator.pop(dialog, false), child: const Text('Cancel')),
            FilledButton(key: const ValueKey('employee-screen-allow'), onPressed: () => Navigator.pop(dialog, true), child: const Text('Allow')),
          ],
        ),
      );
      if (ok != true) return;
    }
    company.send(Commands.setScreenAllowed(m.conversationId, value), retry: false);
  }

  void _showPeople(BuildContext context, String title, List<EmployeeSummary> people) {
    showModalBottomSheet<void>(
      context: context,
      builder: (sheet) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 16, 16, 8), child: Eyebrow(title)),
            for (final p in people)
              ListTile(
                leading: Monogram(p.monogram, size: 32, active: p.working),
                title: Text(p.name),
                subtitle: p.roleLine.isEmpty ? null : Text(p.roleLine),
                onTap: () {
                  Navigator.pop(sheet);
                  context.push('/employee/${p.id}');
                },
              ),
          ],
        ),
      ),
    );
  }
}

/// The server feature that has guidelines (AUR-GL-01); without it the section stays hidden.
const guidelinesFeature = 'guidelines';

/// The guidelines linked to an employee, read-only: what its runs get on top
/// of its instructions. Linking and editing are done in Aurelius on the PC.
class EmployeeGuidelines extends StatefulWidget {
  const EmployeeGuidelines({super.key, required this.employee});

  final EmployeeSummary employee;

  @override
  State<EmployeeGuidelines> createState() => _EmployeeGuidelinesState();
}

class _EmployeeGuidelinesState extends State<EmployeeGuidelines> {
  late Future<EffectiveGuidelines> _set;
  StreamSubscription<Event>? _changes;

  @override
  void initState() {
    super.initState();
    final repository = context.read<AureliusRepository>();
    _set = repository.effectiveGuidelines(widget.employee.id);
    // A guideline edited or linked on the PC shows here at once.
    _changes = repository.guidelineChanges.listen((_) => _reload());
  }

  @override
  void dispose() {
    _changes?.cancel();
    super.dispose();
  }

  void _reload() {
    if (!mounted) return;
    final next = context.read<AureliusRepository>().effectiveGuidelines(widget.employee.id);
    setState(() {
      _set = next;
    });
  }

  @override
  Widget build(BuildContext context) {
    final m = widget.employee;
    return FutureBuilder<EffectiveGuidelines>(
      future: _set,
      builder: (context, snap) {
        final set = snap.data;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            GroupHeader('Guidelines', count: set?.guidelines.length),
            InsetGroup(
              children: [
                if (snap.hasError)
                  GroupRow(
                    key: const ValueKey('employee-guidelines-retry'),
                    title: "Couldn't load the guidelines",
                    subtitle: 'Try again',
                    onTap: _reload,
                  )
                else if (set == null)
                  const GroupRow(title: 'Loading…', chevron: false)
                else if (set.guidelines.isEmpty)
                  GroupRow(title: '${m.name} has no guidelines', subtitle: 'Link them in Aurelius on the PC.', chevron: false)
                else
                  for (final g in set.guidelines)
                    GroupRow(
                      key: ValueKey('employee-guideline-${g.id}'),
                      title: g.title,
                      subtitle: 'Version ${g.version} · ${g.chars} characters',
                      onTap: () => _showText(context, g.title, g.text),
                    ),
              ],
            ),
          ],
        );
      },
    );
  }
}

/// A long read-only text (instructions or a guideline) in a sheet, selectable
/// and shown as written: no Markdown, so nothing in it loads or links.
void _showText(BuildContext context, String title, String text) {
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (sheet) => SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.85),
        child: ListView(
          key: const ValueKey('employee-text-sheet'),
          shrinkWrap: true,
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
          children: [
            Padding(padding: const EdgeInsets.only(bottom: 8), child: Text(title, style: Tokens.title(size: 20))),
            SelectableText(text),
          ],
        ),
      ),
    ),
  );
}

/// Edit's Advanced section, folded as on the desktop: subagents, run one at a
/// time and instruction versions (read-only; restoring is on the PC).
class _Advanced extends StatelessWidget {
  const _Advanced({required this.employee});

  final EmployeeSummary employee;

  @override
  Widget build(BuildContext context) {
    final m = employee;
    final versions = context.read<AureliusRepository>().linkState.has(Harness2.instructionProposals);
    final ownDefaults = context.read<AureliusRepository>().linkState.has(userRunDefaultsFeature);
    return ExpansionTile(
      key: const ValueKey('employee-advanced'),
      tilePadding: const EdgeInsets.symmetric(horizontal: 16),
      childrenPadding: EdgeInsets.zero,
      shape: const Border(),
      collapsedShape: const Border(),
      title: const Text('Advanced'),
      subtitle: Text(
        [if (!ownDefaults) 'Subagents', if (m.sequentialTasks) 'one at a time', if (versions) 'instruction versions'].join(' · '),
        style: TextStyle(color: Tokens.muted, fontSize: 12.5),
      ),
      children: [
        if (!ownDefaults)
          GroupRow(key: const ValueKey('employee-subagents'), title: 'Subagents', subtitle: 'Model of the helpers ${m.name} starts', onTap: () => showEmployeeRunSheet(context, m, subagents: true)),
        EmployeeSequentialTile(employee: m),
        if (versions) GroupRow(key: const ValueKey('employee-instruction-versions'), title: 'Instruction versions', onTap: () => _showInstructionVersions(context, m)),
      ],
    );
  }
}

/// The versions of [m]'s instructions, newest first; read-only on the phone.
void _showInstructionVersions(BuildContext context, EmployeeSummary m) {
  final repository = context.read<AureliusRepository>();
  final future = repository.instructionVersions(m.id);
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (sheet) => SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.85),
        child: FutureBuilder<EmployeeInstructionVersionsSnapshot>(
          future: future,
          builder: (context, snap) => ListView(
            shrinkWrap: true,
            padding: const EdgeInsets.only(bottom: 12),
            children: [
              Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Text('Instructions of ${m.name}', style: Tokens.title(size: 20))),
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                child: Text('Restoring a version is done in Aurelius on the PC.', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
              ),
              if (snap.hasError)
                ListTile(title: Text(snap.error.toString(), style: TextStyle(color: Tokens.muted)))
              else if (snap.data case final data?)
                for (final v in data.versions)
                  ExpansionTile(
                    key: ValueKey('instruction-version-${v.version}'),
                    shape: const Border(),
                    collapsedShape: const Border(),
                    title: Text('Version ${v.version}${v.version == data.currentVersion ? ' · in use' : ''}'),
                    subtitle: Text(
                      [
                        switch (v.source) {
                          'initial' => 'As it was',
                          'created' => 'Created',
                          'edited' => 'Edited',
                          'proposal' => 'Approved proposal',
                          'restored' => v.restoredFrom == null ? 'Restored' : 'Restored from version ${v.restoredFrom}',
                          final other => other,
                        },
                        Fmt.ago(v.createdAtMs),
                      ].join(' · '),
                      style: TextStyle(color: Tokens.muted, fontSize: 12.5),
                    ),
                    childrenPadding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
                    expandedCrossAxisAlignment: CrossAxisAlignment.start,
                    children: [SelectableText(v.instructions.isEmpty ? 'No instructions.' : v.instructions)],
                  )
              else
                const Padding(padding: EdgeInsets.all(24), child: Center(child: CircularProgressIndicator())),
            ],
          ),
        ),
      ),
    ),
  );
}

/// One of an employee's task conversations (writable) or an earlier chat
/// of it (read-only). The employee's own chat has no page: the group chat
/// (Team) took its place.
class ConversationPage extends StatelessWidget {
  const ConversationPage({super.key, required this.conversationId, this.employeeId});

  final String conversationId;

  /// Its employee, when the company does not know the conversation yet (a task
  /// discussion that just started).
  final String? employeeId;

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final task = company.tasks.where((t) => t.conversationId == conversationId).firstOrNull;
    final m = company.employee(task?.employeeId ?? employeeId ?? '') ?? company.employees.where((e) => e.conversations.any((c) => c.id == conversationId)).firstOrNull;
    // An employee's own chat has no page: the group chat (Team) took its place.
    if (company.employees.any((e) => e.conversationId == conversationId)) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (context.mounted) context.go('/sessions');
      });
      return const Scaffold(body: SizedBox.shrink());
    }
    if (m == null && !company.loading) return _UnknownConversation(conversationId: conversationId);
    return Scaffold(
      appBar: GlassAppBar(
        titleSpacing: 0,
        title: Row(
          children: [
            if (m != null) ...[Monogram(m.monogram, size: 32, active: m.working), const SizedBox(width: 10)],
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(task?.title ?? m?.threadTitle(conversationId) ?? m?.name ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: Tokens.title(size: 18)),
                  Text(
                    task != null ? 'Task conversation with ${m?.name ?? ''}' : 'Conversation with ${m?.name ?? 'an employee'}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 12, color: Tokens.muted),
                  ),
                ],
              ),
            ),
          ],
        ),
        actions: [if (m != null) Padding(padding: const EdgeInsets.only(right: 12), child: Center(child: employeePresence(m)))],
      ),
      body: Column(
        children: [
          const ConnectionBanner(),
          Expanded(
            child: m == null
                ? const Center(child: CircularProgressIndicator())
                // Messages go to its employee; one bloc per conversation.
                : BlocProvider(
                    key: ValueKey(conversationId),
                    create: (_) => ChatBloc(context.read<AureliusRepository>(), conversationId: conversationId, employeeId: m.id),
                    child: ChatView(
                      emptyTitle: 'Write to ${m.name}',
                      emptyDetail: m.instructions,
                      // A message can become a ticket (MU2-10): in the task's project, if any.
                      source: TicketSource(from: task != null ? 'Task “${task.title}”' : 'Conversation with ${m.name}', projectId: task?.projectId),
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

/// A conversation that is neither a task's nor an employee's thread. A work
/// session's (an Inbox "Running" row, a link) opens as that session; anything
/// else says it no longer exists.
class _UnknownConversation extends StatefulWidget {
  const _UnknownConversation({required this.conversationId});

  final String conversationId;

  @override
  State<_UnknownConversation> createState() => _UnknownConversationState();
}

class _UnknownConversationState extends State<_UnknownConversation> {
  bool _looking = true;
  bool _failed = false;

  @override
  void initState() {
    super.initState();
    _look();
  }

  Future<void> _look() async {
    final repository = context.read<AureliusRepository>();
    setState(() {
      _looking = true;
      _failed = false;
    });
    var session = false;
    var failed = false;
    if (repository.linkState.has(workSessionsFeature)) {
      try {
        session = (await repository.workSessions()).any((s) => s.conversationId == widget.conversationId);
      } on Object {
        failed = true;
      }
    }
    if (!mounted) return;
    if (session) {
      context.replace('/session/${widget.conversationId}');
      return;
    }
    setState(() {
      _looking = false;
      _failed = failed;
    });
  }

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: const GlassAppBar(title: Text('Conversation')),
        body: _looking
            ? const Center(child: CircularProgressIndicator())
            : _failed
                ? EmptyState('Could not open this conversation.', key: const ValueKey('conversation-load-failed'), action: OutlinedButton(onPressed: _look, child: const Text('Try again')))
                : const EmptyState('This conversation no longer exists', key: ValueKey('conversation-gone')),
      );
}

/// What runs for this employee now: its own steps, workers, subagents and the
/// sub-tasks it delegated. Tap for the full list.
class AgentsBar extends StatelessWidget {
  const AgentsBar({super.key, required this.employee});

  final EmployeeSummary employee;

  String _since(int ms) {
    final minutes = (DateTime.now().millisecondsSinceEpoch - ms) ~/ 60000;
    return minutes <= 0 ? 'just now' : '$minutes min';
  }

  void _open(BuildContext context) {
    final tasks = context.read<CompanyBloc>().state.tasks;
    showModalBottomSheet<void>(
      context: context,
      builder: (sheet) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 16, 16, 8), child: Eyebrow('At work for ${employee.name}')),
            for (final a in employee.agents)
              ListTile(
                contentPadding: EdgeInsets.only(left: a.kind == 'subagent' ? 32 : 16, right: 16),
                leading: Icon(
                  switch (a.kind) {
                    'delegated' => Icons.forward_outlined,
                    'subagent' => Icons.travel_explore,
                    'worker' => Icons.handyman_outlined,
                    'conversation' => Icons.chat_bubble_outline,
                    _ => Icons.psychology_outlined,
                  },
                  color: a.kind == 'delegated' ? Tokens.text2 : Tokens.accent,
                ),
                title: Text('${a.label} · ${a.name}'),
                subtitle: Text([if (a.detail.isNotEmpty) a.detail, _since(a.sinceMs)].join(' · '), style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                trailing: a.taskId == null ? null : const Icon(Icons.chevron_right),
                onTap: switch (tasks.where((t) => t.id == a.taskId).firstOrNull) {
                  null => null,
                  final task => () {
                      Navigator.pop(sheet);
                      context.push('/project/${task.projectId}/task/${task.id}');
                    },
                },
              ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final running = employee.agents.where((a) => a.kind != 'delegated').length;
    final delegated = employee.agents.length - running;
    return InkWell(
      onTap: () => _open(context),
      child: Container(
        width: double.infinity,
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        decoration: BoxDecoration(border: Border(bottom: BorderSide(color: Tokens.border))),
        child: Row(
          children: [
            Icon(Icons.bolt, size: 16, color: Tokens.accent),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                [
                  if (running > 0) '$running at work',
                  if (delegated > 0) '$delegated delegated',
                  employee.agents.map((a) => a.name).take(3).join(', '),
                ].join(' · '),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: 13, color: Tokens.text2),
              ),
            ),
            Icon(Icons.expand_more, size: 18, color: Tokens.muted),
          ],
        ),
      ),
    );
  }
}
