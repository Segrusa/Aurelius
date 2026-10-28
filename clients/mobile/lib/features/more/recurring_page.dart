import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_banner.dart';

String _when(int ms) => DateFormat('EEE d MMM, HH:mm').format(DateTime.fromMillisecondsSinceEpoch(ms));

/// Settings → Advanced → Recurring tasks: every recurring task of the
/// company, read-only, with the project it belongs to. They are made and
/// changed on the desktop.
class RecurringPage extends StatefulWidget {
  const RecurringPage({super.key});

  @override
  State<RecurringPage> createState() => _RecurringPageState();
}

class _RecurringPageState extends State<RecurringPage> {
  late final AureliusRepository _repository = context.read<AureliusRepository>();
  late Future<List<TaskScheduleView>> _schedules = _repository.taskSchedules();

  Future<void> _reload() async {
    final next = _repository.taskSchedules();
    setState(() => _schedules = next);
    try {
      await next;
    } on Object {
      // Shown by the body.
    }
  }

  @override
  Widget build(BuildContext context) {
    final projects = context.watch<CompanyBloc>().state.projects;
    String projectName(String id) => projects.where((p) => p.id == id).firstOrNull?.name ?? 'Project';
    return Scaffold(
      appBar: const GlassAppBar(
        title: Text('Recurring tasks'),
        actions: [InfoIcon('Made and changed in Aurelius on the desktop.', label: 'Recurring tasks')],
      ),
      body: Column(
        children: [
          const ConnectionBanner(),
          Expanded(
            child: FutureBuilder<List<TaskScheduleView>>(
              future: _schedules,
              builder: (context, snapshot) {
                if (snapshot.hasError) {
                  return RefreshIndicator.adaptive(
                    onRefresh: _reload,
                    child: ListView(
                      physics: const AlwaysScrollableScrollPhysics(),
                      padding: const EdgeInsets.all(24),
                      children: [
                        const SizedBox(height: 80),
                        Text('Recurring tasks could not be loaded.', key: const ValueKey('recurring-error'), textAlign: TextAlign.center, style: TextStyle(color: Tokens.muted)),
                        const SizedBox(height: 16),
                        Center(child: OutlinedButton(onPressed: _reload, child: const Text('Try again'))),
                      ],
                    ),
                  );
                }
                final schedules = snapshot.data;
                if (schedules == null) return const Center(child: CircularProgressIndicator());
                return RefreshIndicator.adaptive(
                  onRefresh: _reload,
                  child: ListView(
                    physics: const AlwaysScrollableScrollPhysics(parent: BouncingScrollPhysics()),
                    padding: EdgeInsets.fromLTRB(0, 12, 0, 32 + MediaQuery.paddingOf(context).bottom),
                    children: [
                      if (schedules.isEmpty)
                        const Padding(padding: EdgeInsets.only(top: 80), child: EmptyState('No recurring tasks'))
                      else
                        InsetGroup(
                          key: const ValueKey('recurring-list'),
                          children: [
                            for (final s in schedules)
                              GroupRow(
                                key: ValueKey('recurring-${s.id}'),
                                title: s.title,
                                titleLines: 2,
                                subtitle: [
                                  'Project: ${projectName(s.projectId)} · ${s.employeeName} · ${s.recurrence.label}',
                                  s.paused ? 'Paused' : (s.nextRunMs == null ? 'No next run' : 'Next: ${_when(s.nextRunMs!)}'),
                                ].join('\n'),
                                chevron: s.lastTaskId != null,
                                onTap: s.lastTaskId == null ? null : () => context.push('/project/${s.projectId}/task/${s.lastTaskId}'),
                              ),
                          ],
                        ),
                    ],
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}
