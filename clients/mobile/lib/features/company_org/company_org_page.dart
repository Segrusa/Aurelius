import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/refresh.dart';
import '../../core/widgets/widgets.dart';
import '../../data/commands.dart';
import '../../data/company.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'employee_widgets.dart';

/// What the Company tab shows.
enum CompanyView { employees, chart, departments }

/// The Company tab: Employees · Org chart · Departments.
class CompanyOrgPage extends StatefulWidget {
  const CompanyOrgPage({super.key, this.initial});

  /// `employees`, `chart` or `departments` (the `view` query parameter).
  final String? initial;

  @override
  State<CompanyOrgPage> createState() => _CompanyOrgPageState();
}

class _CompanyOrgPageState extends State<CompanyOrgPage> {
  late CompanyView _view = CompanyView.values.where((v) => v.name == widget.initial).firstOrNull ?? CompanyView.employees;

  @override
  void didUpdateWidget(CompanyOrgPage old) {
    super.didUpdateWidget(old);
    // Home's "Org chart" link goes here with ?view=chart.
    final asked = CompanyView.values.where((v) => v.name == widget.initial).firstOrNull;
    if (asked != null && widget.initial != old.initial) _view = asked;
  }

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<CompanyBloc, CompanyState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<CompanyBloc>().add(const CompanyNoticeShown());
      },
      builder: (context, state) {
        final bloc = context.read<CompanyBloc>();
        final employees = state.activeEmployees;
        return LargeTitleScaffold(
          title: 'Company',
          subtitle: [
            '${employees.length} employee${employees.length == 1 ? '' : 's'}',
            if (state.activeDepartments.isNotEmpty) '${state.activeDepartments.length} department${state.activeDepartments.length == 1 ? '' : 's'}',
            if (state.working > 0) '${state.working} working',
          ].join(' · '),
          actions: [
            GlassButton(
              tooltip: _view == CompanyView.departments ? 'New department' : 'New employee',
              icon: CupertinoIcons.add,
              onPressed: () => _view == CompanyView.departments ? createDepartment(context) : createEmployee(context),
            ),
          ],
          onRefresh: () => untilReloaded(bloc, () => bloc.add(const CompanyRefreshed()), busy: (s) => s.loading),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
              child: PillSegments<CompanyView>(
                segments: const [(CompanyView.employees, 'Employees'), (CompanyView.chart, 'Org chart'), (CompanyView.departments, 'Departments')],
                selected: _view,
                onChanged: (v) => setState(() => _view = v),
              ),
            ),
            if (state.loading && state.employees.isEmpty)
              const Padding(padding: EdgeInsets.only(top: 48), child: Center(child: CupertinoActivityIndicator()))
            else
              ...switch (_view) {
                CompanyView.employees => _employees(context, state),
                CompanyView.chart => _chart(context, state),
                CompanyView.departments => _departments(context, state),
              },
          ],
        );
      },
    );
  }

  List<Widget> _employees(BuildContext context, CompanyState state) {
    if (state.activeEmployees.isEmpty) {
      return [
        Padding(
          padding: const EdgeInsets.only(top: 40),
          child: EmptyState('No employees yet', action: FilledButton(onPressed: () => createEmployee(context), child: const Text('New employee'))),
        ),
      ];
    }
    return [
      if (state.pendingProposals.isNotEmpty) ...[
        GroupHeader('Proposals', count: state.pendingProposals.length),
        Padding(padding: const EdgeInsets.symmetric(horizontal: 16), child: ProposalList(proposals: state.pendingProposals)),
      ],
      for (final g in pickerGroups(state.employees, state.activeDepartments)) ...[
        GroupHeader(g.title, count: g.employees.length),
        InsetGroup(
          dividerIndent: 70,
          children: [for (final m in g.employees) EmployeeRow(key: ValueKey('employee-${m.id}'), employee: m, state: state, showDepartment: false)],
        ),
      ],
    ];
  }

  List<Widget> _chart(BuildContext context, CompanyState state) => [
        const SizedBox(height: 12),
        if (state.activeEmployees.isEmpty)
          const EmptyState('No employees yet')
        else
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: OrgChartCard(employees: state.activeEmployees, departments: state.departments),
          ),
      ];

  List<Widget> _departments(BuildContext context, CompanyState state) {
    final departments = state.activeDepartments;
    if (departments.isEmpty) {
      return [
        Padding(
          padding: const EdgeInsets.only(top: 40),
          child: EmptyState(
            'No departments yet',
            detail: 'A department groups employees, with a head and its own memory.',
            action: FilledButton(onPressed: () => createDepartment(context), child: const Text('New department')),
          ),
        ),
      ];
    }
    return [
      const SizedBox(height: 8),
      InsetGroup(
        children: [
          for (final d in departments)
            GroupRow(
              key: ValueKey('department-${d.id}'),
              onTap: () => context.push('/department/${d.id}'),
              leading: LetterTile(d.name.isEmpty ? '?' : d.name.characters.first.toUpperCase()),
              title: d.name,
              subtitle: [
                if (state.employee(d.headId ?? '') case final head?) 'Head: ${head.name}',
                '${d.memberCount} member${d.memberCount == 1 ? '' : 's'}',
                if (d.openTasks > 0) '${d.openTasks} open task${d.openTasks == 1 ? '' : 's'}',
              ].join(' · '),
              chevron: true,
            ),
        ],
      ),
    ];
  }
}

/// "New department": name, description and head.
Future<void> createDepartment(BuildContext context) async {
  final bloc = context.read<CompanyBloc>();
  final values = await promptForm(
    context,
    title: 'New department',
    action: 'Create',
    fields: [
      const FieldSpec('Name', hint: 'For example Engineering'),
      const FieldSpec('Description', required: false, lines: 2),
      FieldSpec('Head', required: false, options: {'': 'Nobody yet', for (final m in bloc.state.activeEmployees) m.id: m.name}, dropdown: true),
    ],
  );
  if (values == null) return;
  bloc.send(
    Commands.createDepartment(values[0], description: values[1].isEmpty ? null : values[1], headId: values[2].isEmpty ? null : values[2]),
    done: 'Department created.',
  );
}

/// Edit a department's name, description and head.
Future<void> editDepartment(BuildContext context, DepartmentSummary d) async {
  final bloc = context.read<CompanyBloc>();
  final values = await promptForm(
    context,
    title: 'Edit ${d.name}',
    fields: [
      FieldSpec('Name', initial: d.name),
      FieldSpec('Description', initial: d.description ?? '', required: false, lines: 2),
      FieldSpec('Head', initial: d.headId ?? '', required: false, options: {'': 'Nobody', for (final m in bloc.state.activeEmployees) m.id: m.name}, dropdown: true),
    ],
  );
  if (values == null) return;
  bloc.send(
    Commands.updateDepartment(d.id, values[0], description: values[1], headId: values[2].isEmpty ? null : values[2]),
    expectedVersion: d.version,
    done: 'Saved.',
  );
}
