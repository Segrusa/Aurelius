import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/widgets/ios.dart';
import '../../core/widgets/refresh.dart';
import '../../core/widgets/widgets.dart';
import '../../data/company.dart';
import '../company/company_bloc.dart';
import 'employee_widgets.dart';

/// What the Company tab shows.
enum CompanyOrgView { employees, chart }

/// The Company tab: Employees · Org chart.
class CompanyOrgPage extends StatefulWidget {
  const CompanyOrgPage({super.key, this.initial});

  /// `employees` or `chart` (the `view` query parameter).
  final String? initial;

  @override
  State<CompanyOrgPage> createState() => _CompanyOrgPageState();
}

class _CompanyOrgPageState extends State<CompanyOrgPage> {
  late CompanyOrgView _view = CompanyOrgView.values.where((v) => v.name == widget.initial).firstOrNull ?? CompanyOrgView.employees;

  @override
  void didUpdateWidget(CompanyOrgPage old) {
    super.didUpdateWidget(old);
    // A link with ?view=chart opens the org chart.
    final asked = CompanyOrgView.values.where((v) => v.name == widget.initial).firstOrNull;
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
            if (state.working > 0) '${state.working} working',
          ].join(' · '),
          actions: [
            GlassButton(
              tooltip: 'New employee',
              icon: CupertinoIcons.add,
              onPressed: () => createEmployee(context),
            ),
          ],
          onRefresh: () => untilReloaded(bloc, () => bloc.add(const CompanyRefreshed()), busy: (s) => s.loading),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
              child: PillSegments<CompanyOrgView>(
                segments: const [(CompanyOrgView.employees, 'Employees'), (CompanyOrgView.chart, 'Org chart')],
                selected: _view,
                onChanged: (v) => setState(() => _view = v),
              ),
            ),
            if (state.loading && state.employees.isEmpty)
              const Padding(padding: EdgeInsets.only(top: 48), child: Center(child: CupertinoActivityIndicator()))
            else
              ...switch (_view) {
                CompanyOrgView.employees => _employees(context, state),
                CompanyOrgView.chart => _chart(context, state),
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
      for (final g in pickerGroups(state.employees)) ...[
        GroupHeader(g.title, count: g.employees.length),
        InsetGroup(
          dividerIndent: 70,
          children: [for (final m in g.employees) EmployeeRow(key: ValueKey('employee-${m.id}'), employee: m, state: state)],
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
            child: OrgChartCard(employees: state.activeEmployees),
          ),
      ];
}
