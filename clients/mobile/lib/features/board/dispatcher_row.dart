import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import 'ticket_actions.dart';

/// The project's dispatcher (§12.2, MU1-11): "Company default (name)" or an
/// active employee of its own; the note says what happens when the chosen one
/// is paused or archived (AC3: nobody takes over, B5 strict) or when nobody
/// dispatches (B5).
class ProjectDispatcherRow extends StatelessWidget {
  const ProjectDispatcherRow({super.key, required this.project});

  final ProjectSummary project;

  Future<void> _pick(BuildContext context, CompanyState company) async {
    final active = company.employees.where((e) => e.status == LifecycleStatus.active).toList()..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    final fallback = employeeName(company, company.company?.dispatcherEmployeeId);
    final current = project.dispatcherEmployeeId;
    final picked = await showModalBottomSheet<(String?,)>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(shrinkWrap: true, children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Dispatcher of ${project.name}')),
            ListTile(
              key: const ValueKey('project-dispatcher-default'),
              title: Text('Company default (${fallback ?? 'none'})'),
              trailing: current == null ? Icon(Icons.check, color: Tokens.accentText) : null,
              onTap: () => Navigator.pop(sheet, (null,)),
            ),
            for (final e in active)
              ListTile(key: ValueKey('project-dispatcher-${e.id}'), title: Text(e.name), subtitle: Text(e.role), trailing: current == e.id ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (e.id,))),
          ]),
        ),
      ),
    );
    if (picked == null || picked.$1 == current || !context.mounted) return;
    final name = employeeName(company, picked.$1);
    context.read<CompanyBloc>().send(
          projectUpdate(UpdateProject(projectId: project.id, dispatcher: SetDispatcher(employeeId: picked.$1))),
          expectedVersion: project.version,
          retry: false,
          done: name == null ? '${project.name} uses the company default again.' : '$name dispatches ${project.name}.',
        );
  }

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final online = context.select<ConnectionBloc, bool>((b) => b.state.isOnline);
    final own = project.dispatcherEmployeeId;
    final ownName = employeeName(company, own);
    final fallback = employeeName(company, company.company?.dispatcherEmployeeId);
    final effective = dispatcherName(company, project.id);
    final inactive = inactiveDispatcher(company, project.id);
    final note = inactive != null
        ? '◆ $inactive. Started tickets wait in Todo until you choose an owner${own == null ? '' : '; the company default does not take over'}.'
        : effective == null
            ? 'No dispatcher. Started tickets wait in Todo until you choose an owner.'
            : 'Chooses an owner when a ticket of this project starts without one.';
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      InsetGroup(children: [
        GroupRow(
          key: const ValueKey('project-dispatcher'),
          leading: const IconTile(CupertinoIcons.shuffle),
          title: 'Dispatcher',
          subtitle: own == null ? 'Company default (${fallback ?? 'none'})' : ownName ?? 'Unknown',
          chevron: online,
          onTap: online ? () => _pick(context, company) : null,
        ),
      ]),
      Padding(
        padding: const EdgeInsets.fromLTRB(32, 6, 32, 0),
        child: Text(note, key: const ValueKey('project-dispatcher-note'), style: TextStyle(fontSize: 13, color: effective == null ? Tokens.warning : Tokens.label2)),
      ),
    ]);
  }
}
