import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/commands.dart';
import '../../data/org_chart.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../inbox/instruction_proposal_card.dart';

/// Paused / Working / Available, as on the desktop cards.
Widget employeePresence(EmployeeSummary m) {
  if (m.status == LifecycleStatus.paused) return Text('Paused', style: TextStyle(color: Tokens.warning, fontSize: 15));
  return Row(mainAxisSize: MainAxisSize.min, children: [
    StatusDot(pulse: m.working, size: 7),
    const SizedBox(width: 6),
    Text(m.working ? 'Working' : 'Available', style: TextStyle(color: m.working ? Tokens.accent : Tokens.label2, fontSize: 15)),
  ]);
}

/// One employee in a list: monogram, name, what it does now (or its role) and
/// its presence. Opens the employee page.
class EmployeeRow extends StatelessWidget {
  const EmployeeRow({super.key, required this.employee, required this.state});

  final EmployeeSummary employee;
  final CompanyState state;

  @override
  Widget build(BuildContext context) {
    final m = employee;
    final current = state.currentTask(m.id);
    return GroupRow(
      onTap: () => context.push('/employee/${m.id}'),
      leading: Monogram(m.monogram, size: 40, active: m.working),
      title: m.name,
      subtitle: current != null
          ? [current.title, state.project(current.projectId)?.name ?? ''].where((s) => s.isNotEmpty).join(' · ')
          : m.roleLine,
      trailing: employeePresence(m),
    );
  }
}

/// "Nobody" plus the employees who can be the boss (value: employee id, '' for nobody).
Map<String, String> reportsToOptions(List<EmployeeSummary> employees) => {
      '': 'Nobody (management team)',
      for (final m in employees) m.id: m.roleLine.isEmpty ? m.name : '${m.name} · ${m.roleLine}',
    };

/// "New employee": name, role, title, boss and instructions.
Future<void> createEmployee(BuildContext context) async {
  final bloc = context.read<CompanyBloc>();
  final state = bloc.state;
  final values = await promptForm(
    context,
    title: 'New employee',
    action: 'Create',
    fields: [
      const FieldSpec('Name', hint: 'For example Atlas'),
      const FieldSpec('Role', hint: 'For example Marketing lead'),
      const FieldSpec('Title', hint: 'For example Head of marketing', required: false),
      FieldSpec('Reports to', required: false, options: reportsToOptions(state.activeEmployees), dropdown: true),
      const FieldSpec('Instructions', required: false, lines: 3),
    ],
  );
  if (values == null) return;
  String? opt(String v) => v.isEmpty ? null : v;
  bloc.send(
    Commands.createEmployee(values[0], values[1], title: opt(values[2]), reportsTo: opt(values[3]), instructions: opt(values[4])),
    done: '${values[0]} joined the company.',
  );
}

/// Edit, pause/resume, archive. Model and effort are on
/// the employee page's Setup rows only.
class EmployeeMenu extends StatelessWidget {
  const EmployeeMenu({super.key, required this.employee});

  final EmployeeSummary employee;

  @override
  Widget build(BuildContext context) {
    final m = employee;
    final bloc = context.read<CompanyBloc>();
    return GlassMenuButton<String>(
      tooltip: 'Employee actions',
      onSelected: (value) async {
        switch (value) {
          case 'edit':
            await editEmployee(context, m);
          case 'pause':
            bloc.send(Commands.setEmployeeStatus(m.id, LifecycleStatus.paused), done: '${m.name} paused.');
          case 'resume':
            bloc.send(Commands.setEmployeeStatus(m.id, LifecycleStatus.active), done: '${m.name} resumed.');
          case 'archive':
            if (await confirm(context, title: 'Archive ${m.name}?', body: 'The employee stops; its history stays.', action: 'Archive', danger: true)) {
              bloc.send(Commands.setEmployeeStatus(m.id, LifecycleStatus.archived), done: '${m.name} archived.', retry: false);
            }
        }
      },
      items: () => [
        const GlassMenuItem('edit', 'Edit'),
        if (m.status == LifecycleStatus.active) const GlassMenuItem('pause', 'Pause'),
        if (m.status == LifecycleStatus.paused) const GlassMenuItem('resume', 'Resume'),
        const GlassMenuItem('archive', 'Archive', danger: true),
      ],
    );
  }
}

/// Name, role, title, boss and instructions of [m]. The org chart position is
/// a separate write after the edit.
Future<void> editEmployee(BuildContext context, EmployeeSummary m) async {
  final bloc = context.read<CompanyBloc>();
  final state = bloc.state;
  final others = possibleBosses(state.activeEmployees, m);
  final values = await promptForm(context, title: 'Edit ${m.name}', fields: [
    FieldSpec('Name', initial: m.name),
    FieldSpec('Role', initial: m.role),
    FieldSpec('Title', initial: m.title ?? '', required: false, hint: 'For example Head of marketing'),
    FieldSpec('Reports to', initial: m.reportsTo ?? '', required: false, options: reportsToOptions(others), dropdown: true),
    FieldSpec('Instructions', initial: m.instructions ?? '', required: false, lines: 3),
  ]);
  if (values == null) return;
  // An empty title clears it.
  bloc.send(
    Commands.updateEmployee(m.id, values[0], values[1], title: values[2], instructions: values[4].isEmpty ? null : values[4]),
    expectedVersion: m.version,
    done: 'Saved.',
    then: [
      if (values[3] != (m.reportsTo ?? '')) Commands.setReportsTo(m.id, values[3].isEmpty ? null : values[3]),
    ],
  );
}

/// The whole company as a tree: each employee under the one it reports to,
/// the management team at the top.
class OrgChartCard extends StatelessWidget {
  const OrgChartCard({super.key, required this.employees});

  final List<EmployeeSummary> employees;

  @override
  Widget build(BuildContext context) {
    return SurfaceCard(
      key: const ValueKey('org-chart'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final node in orgChart(employees))
            InkWell(
              key: ValueKey('org-node-${node.employee.id}'),
              onTap: () => context.push('/employee/${node.employee.id}'),
              child: Padding(
                padding: EdgeInsets.only(left: node.depth * 22.0, top: 6, bottom: 6),
                child: Row(
                  children: [
                    if (node.depth > 0) Padding(padding: const EdgeInsets.only(right: 6), child: Icon(Icons.subdirectory_arrow_right, size: 16, color: Tokens.muted)),
                    Monogram(node.employee.monogram, active: node.employee.working, size: 28),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(node.employee.name, style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600)),
                          if (node.employee.roleLine.isNotEmpty)
                            Text(node.employee.roleLine, style: TextStyle(color: Tokens.text2, fontSize: 12.5)),
                        ],
                      ),
                    ),
                    StatusDot(pulse: node.employee.working, size: 7),
                  ],
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// Pending proposals of employees: approve or decline each.
class ProposalList extends StatelessWidget {
  const ProposalList({super.key, required this.proposals});

  final List<ProposalView> proposals;

  @override
  Widget build(BuildContext context) {
    final bloc = context.read<CompanyBloc>();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final p in proposals)
          Padding(
            padding: const EdgeInsets.only(bottom: 10),
            child: ProposalCard(
              proposal: p,
              onDecide: (approve, note, instructions) => bloc.send(
                Commands.decideProposal(p.id, approve, note: note, instructions: instructions),
                done: switch ((p.isInstructions, approve)) {
                  (true, true) => 'New instructions for ${p.targetEmployeeName ?? 'the employee'} are saved.',
                  (true, false) => 'Rejected.',
                  (false, true) => '${p.name} joined the company.',
                  (false, false) => 'Declined.',
                },
                retry: false,
              ),
            ),
          ),
      ],
    );
  }
}

/// A colleague an employee proposes: approve (it joins the company) or decline with a note.
class ProposalCard extends StatelessWidget {
  const ProposalCard({super.key, required this.proposal, required this.onDecide});

  final ProposalView proposal;

  /// The user's choice; a decline may carry a note for the proposing employee,
  /// approved instructions the user's edited text.
  final void Function(bool approve, String? note, String? instructions) onDecide;

  @override
  Widget build(BuildContext context) {
    final p = proposal;
    if (p.isInstructions) return InstructionProposalCard(proposal: p, onDecide: onDecide);
    final meta = [
      if ((p.title ?? '').isNotEmpty) p.title!,
      p.role,
      if (p.reportsToName != null) 'reports to ${p.reportsToName}',
    ].join(' · ');
    return SurfaceCard(
      key: ValueKey('proposal-${p.id}'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(p.name, style: Tokens.title(size: 18)),
          Text(meta, style: TextStyle(color: Tokens.text2, fontSize: 13)),
          const SizedBox(height: 4),
          Text('Proposed by ${p.proposedByName}', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
          if (p.reason.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(p.reason, style: const TextStyle(fontSize: 14)),
          ],
          if (p.instructions.isNotEmpty) ...[
            const SizedBox(height: 6),
            Text(p.instructions, maxLines: 3, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.text2, fontSize: 13)),
          ],
          const SizedBox(height: 10),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton(
                key: ValueKey('decline-${p.id}'),
                onPressed: () async {
                  final note = await promptForm(context, title: 'Decline ${p.name}?', action: 'Decline', fields: const [FieldSpec('Note for the employee', required: false, lines: 2)]);
                  if (note == null) return;
                  onDecide(false, note.first.isEmpty ? null : note.first, null);
                },
                child: const Text('Decline'),
              ),
              const SizedBox(width: 8),
              FilledButton(
                key: ValueKey('approve-${p.id}'),
                onPressed: () => onDecide(true, null, null),
                child: const Text('Approve'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
