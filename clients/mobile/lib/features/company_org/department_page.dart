import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/commands.dart';
import '../../data/company.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_banner.dart';
import '../project/tasks_tab.dart';
import 'company_org_page.dart';
import 'department_memory.dart';
import 'employee_widgets.dart';

/// A department: description, head, members, their open tasks and the
/// department's own memory.
class DepartmentPage extends StatelessWidget {
  const DepartmentPage({super.key, required this.departmentId});

  final String departmentId;

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<CompanyBloc, CompanyState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<CompanyBloc>().add(const CompanyNoticeShown());
      },
      builder: (context, state) {
        final d = state.department(departmentId);
        if (d == null) {
          return Scaffold(
            appBar: GlassAppBar(),
            body: state.loading ? const Center(child: CircularProgressIndicator()) : const EmptyState('This department no longer exists'),
          );
        }
        final members = departmentMembers(state.employees, d);
        final ids = {for (final m in members) m.id};
        final open = state.tasks.where((t) => ids.contains(t.employeeId) && t.isOpen).toList()..sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs));
        final head = state.employee(d.headId ?? '');
        return Scaffold(
          appBar: GlassAppBar(
            title: Text(d.name, maxLines: 1, overflow: TextOverflow.ellipsis),
            actions: [GlassCapsule(children: [_DepartmentMenu(department: d)])],
          ),
          body: Column(
            children: [
              const ConnectionBanner(),
              Expanded(
                child: ListView(
                  padding: EdgeInsets.only(bottom: listBottom(context)),
                  children: [
                    if ((d.description ?? '').isNotEmpty)
                      Padding(padding: const EdgeInsets.fromLTRB(32, 12, 32, 0), child: Text(d.description!, style: TextStyle(fontSize: 15, color: Tokens.label2))),
                    Padding(
                      padding: const EdgeInsets.fromLTRB(32, 6, 32, 0),
                      child: Text(head == null ? 'No head' : 'Head: ${head.name}', style: TextStyle(fontSize: 13, color: Tokens.label3)),
                    ),
                    GroupHeader('Members', count: members.length, action: 'Add', onAction: () => createEmployee(context, departmentId: d.id)),
                    InsetGroup(
                      dividerIndent: 70,
                      children: [
                        if (members.isEmpty) const GroupRow(title: 'Nobody in this department yet', chevron: false),
                        for (final m in members) EmployeeRow(key: ValueKey('member-${m.id}'), employee: m, state: state, showDepartment: false),
                      ],
                    ),
                    GroupHeader('Open Tasks', count: open.length),
                    InsetGroup(
                      children: [
                        if (open.isEmpty) const GroupRow(title: 'No open tasks', chevron: false),
                        for (final t in open.take(10)) TaskRow(task: t),
                      ],
                    ),
                    DepartmentMemory(departmentId: d.id),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

class _DepartmentMenu extends StatelessWidget {
  const _DepartmentMenu({required this.department});

  final DepartmentSummary department;

  @override
  Widget build(BuildContext context) {
    final d = department;
    final bloc = context.read<CompanyBloc>();
    return GlassMenuButton<String>(
      tooltip: 'Department actions',
      icon: CupertinoIcons.ellipsis,
      onSelected: (value) async {
        switch (value) {
          case 'edit':
            await editDepartment(context, d);
          case 'add':
            await createEmployee(context, departmentId: d.id);
          case 'archive':
            if (await confirm(context, title: 'Archive ${d.name}?', body: 'Its employees keep working, without a department.', action: 'Archive', danger: true)) {
              bloc.send(Commands.archiveDepartment(d.id), done: '${d.name} archived.', retry: false);
              if (context.mounted) context.pop();
            }
        }
      },
      items: () => const [
        GlassMenuItem('edit', 'Edit'),
        GlassMenuItem('add', 'New employee here'),
        GlassMenuItem('archive', 'Archive', danger: true),
      ],
    );
  }
}
