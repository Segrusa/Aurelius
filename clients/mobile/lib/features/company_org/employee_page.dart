import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/company.dart';
import '../../data/views.dart';
import '../chat/chat_bloc.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';
import '../connection/connection_banner.dart';
import '../dashboard/give_work.dart';
import 'employee_run_sheet.dart';
import '../plugins/employee_plugins_sheet.dart';
import '../project/tasks_tab.dart';
import 'employee_widgets.dart';

/// An employee: give it work, where it sits (department, boss, team), where it
/// works now, its model, plugins and status, and its tasks across projects.
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
        final department = company.department(m.departmentId);
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
                        [m.roleLine, department?.name ?? ''].where((s) => s.isNotEmpty).join(' · '),
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
                    if ((m.instructions ?? '').isNotEmpty)
                      Padding(
                        padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                        child: Text(m.instructions!, maxLines: 3, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 14, color: Tokens.label2)),
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
                          key: const ValueKey('employee-department'),
                          title: 'Department',
                          value: department?.name ?? 'None',
                          onTap: department == null ? () => editEmployee(context, m) : () => context.push('/department/${department.id}'),
                        ),
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
                          GroupRow(
                            key: const ValueKey('employee-model'),
                            title: 'Model and effort',
                            onTap: () => showEmployeeRunSheet(context, m),
                          ),
                          GroupRow(title: 'Mode', value: m.agentMode.label, chevron: false),
                          GroupRow(title: heartbeatLine(m.heartbeat), chevron: false),
                          GroupRow(key: const ValueKey('employee-edit'), title: 'Edit', onTap: () => editEmployee(context, m)),
                          _Advanced(employee: m),
                        ],
                      ),
                    ],
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

/// Edit's Advanced section, folded as on the desktop: subagents, run one at a
/// time, plugins and instruction versions (read-only; restoring is on the PC).
class _Advanced extends StatelessWidget {
  const _Advanced({required this.employee});

  final EmployeeSummary employee;

  @override
  Widget build(BuildContext context) {
    final m = employee;
    final versions = context.read<AureliusRepository>().linkState.has(Harness2.instructionProposals);
    return ExpansionTile(
      key: const ValueKey('employee-advanced'),
      tilePadding: const EdgeInsets.symmetric(horizontal: 16),
      childrenPadding: EdgeInsets.zero,
      shape: const Border(),
      collapsedShape: const Border(),
      title: const Text('Advanced'),
      subtitle: Text(
        ['Subagents', if (m.sequentialTasks) 'one at a time', 'plugins', if (versions) 'instruction versions'].join(' · '),
        style: TextStyle(color: Tokens.muted, fontSize: 12.5),
      ),
      children: [
        GroupRow(key: const ValueKey('employee-subagents'), title: 'Subagents', subtitle: 'Model of the helpers ${m.name} starts', onTap: () => showEmployeeRunSheet(context, m, subagents: true)),
        EmployeeSequentialTile(employee: m),
        GroupRow(key: const ValueKey('employee-plugins'), title: 'Plugins', onTap: () => showEmployeePluginsSheet(context, m)),
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
                ? (company.loading ? const Center(child: CircularProgressIndicator()) : const EmptyState('This conversation no longer exists'))
                // Messages go to its employee; one bloc per conversation.
                : BlocProvider(
                    key: ValueKey(conversationId),
                    create: (_) => ChatBloc(context.read<AureliusRepository>(), conversationId: conversationId, employeeId: m.id),
                    child: ChatView(emptyTitle: 'Write to ${m.name}', emptyDetail: m.instructions),
                  ),
          ),
        ],
      ),
    );
  }
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
